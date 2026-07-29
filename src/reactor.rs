use std::os::unix::io::RawFd;
use std::sync::{Arc, Mutex};
use std::collections::HashMap;

#[repr(C)]
#[cfg_attr(target_arch = "x86_64", repr(packed))]
pub struct EpollEvent {
    pub events: u32,
    pub data: u64,
}

pub const EPOLLIN: u32 = 0x001;
pub const EPOLLOUT: u32 = 0x004;
pub const EPOLLERR: u32 = 0x008;
pub const EPOLLHUP: u32 = 0x010;
pub const EPOLLONESHOT: u32 = 1 << 30;
pub const EPOLLET: u32 = 1 << 31;

pub const EPOLL_CTL_ADD: i32 = 1;
pub const EPOLL_CTL_DEL: i32 = 2;
pub const EPOLL_CTL_MOD: i32 = 3;

extern "C" {
    fn epoll_create1(flags: i32) -> i32;
    fn epoll_ctl(epfd: i32, op: i32, fd: i32, event: *mut EpollEvent) -> i32;
    fn epoll_wait(epfd: i32, events: *mut EpollEvent, maxevents: i32, timeout: i32) -> i32;
    fn close(fd: i32) -> i32;
}

pub type EventCallback = Box<dyn Fn(u32, u64) + Send + Sync + 'static>;

/// Entrada interna do Reactor, associa a callback à sua geração.
/// A geração é incrementada a cada novo register() no mesmo fd, permitindo
/// que um unregister() atrasado (vindo de uma conexão antiga já fechada)
/// não remova acidentalmente a callback de uma conexão nova que reutilizou o mesmo fd.
struct CallbackEntry {
    callback: EventCallback,
    generation: u64,
}

pub struct Reactor {
    epoll_fd: RawFd,
    /// Mapeia fd -> (callback, geração atual).
    entries: Arc<Mutex<HashMap<RawFd, CallbackEntry>>>,
}

impl Reactor {
    pub fn new() -> Result<Self, std::io::Error> {
        let epoll_fd = unsafe { epoll_create1(0) };
        if epoll_fd < 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(Self {
            epoll_fd,
            entries: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    /// Registra um fd no epoll com a callback fornecida.
    /// Retorna a geração atribuída a essa entrada — guarde-a e passe-a
    /// para `unregister_generation` ao desregistrar, para evitar o race
    /// condition por reutilização de fd pelo kernel.
    pub fn register<F>(&self, fd: RawFd, interest: u32, callback: F) -> Result<u64, std::io::Error>
    where
        F: Fn(u32, u64) + Send + Sync + 'static,
    {
        // 1. Insere no HashMap PRIMEIRO para evitar Race Condition.
        // Se epoll_ctl rodasse antes, epoll_wait poderia disparar em outra thread 
        // ANTES de inserirmos a callback no HashMap, perdendo o evento (EPOLLONESHOT) para sempre.
        let gen = if let Ok(mut map) = self.entries.lock() {
            let next_gen = map.get(&fd).map(|e| e.generation + 1).unwrap_or(1);
            map.insert(fd, CallbackEntry {
                callback: Box::new(callback),
                generation: next_gen,
            });
            next_gen
        } else {
            return Err(std::io::Error::new(std::io::ErrorKind::Other, "Falha ao obter lock do reactor"));
        };

        // 2. Só então adiciona no epoll
        let mut event = EpollEvent {
            events: interest | EPOLLET,
            data: fd as u64,
        };

        let res = unsafe { epoll_ctl(self.epoll_fd, EPOLL_CTL_ADD, fd, &mut event) };
        if res < 0 {
            let err = std::io::Error::last_os_error();
            // Desfaz a inserção no HashMap em caso de falha no SO
            if let Ok(mut map) = self.entries.lock() {
                if map.get(&fd).map(|e| e.generation == gen).unwrap_or(false) {
                    map.remove(&fd);
                }
            }
            return Err(err);
        }

        Ok(gen)
    }

    /// Desregistra o fd do epoll apenas se a geração atual do fd bater com
    /// `expected_gen`. Isso previne que um unregister() atrasado (de uma
    /// conexão já fechada) remova a callback de uma nova conexão que o Linux
    /// reutilizou no mesmo fd — a causa raiz do bug de travamento intermitente.
    pub fn unregister_generation(&self, fd: RawFd, expected_gen: u64) -> Result<(), std::io::Error> {
        let should_remove = if let Ok(map) = self.entries.lock() {
            map.get(&fd).map(|e| e.generation == expected_gen).unwrap_or(false)
        } else {
            false
        };

        if !should_remove {
            // Geração não bate: outra conexão já tomou esse fd. Não fazemos nada.
            return Ok(());
        }

        let mut event = EpollEvent { events: 0, data: fd as u64 };
        unsafe { epoll_ctl(self.epoll_fd, EPOLL_CTL_DEL, fd, &mut event) };

        if let Ok(mut map) = self.entries.lock() {
            // Só remove se a geração ainda for a mesma (double-check após o lock).
            if map.get(&fd).map(|e| e.generation == expected_gen).unwrap_or(false) {
                map.remove(&fd);
            }
        }

        Ok(())
    }

    /// Desregistra o fd incondicionalmente. Use apenas em situações onde a
    /// identidade da conexão é garantida (ex: Graceful Shutdown, upgrade HTTP→WS
    /// onde o chamador detém o fd e sabe que ninguém mais o reutilizou).
    pub fn unregister(&self, fd: RawFd) -> Result<(), std::io::Error> {
        let mut event = EpollEvent { events: 0, data: fd as u64 };
        unsafe { epoll_ctl(self.epoll_fd, EPOLL_CTL_DEL, fd, &mut event) };
        if let Ok(mut map) = self.entries.lock() {
            map.remove(&fd);
        }
        Ok(())
    }

    pub fn modify(&self, fd: RawFd, interest: u32) -> Result<(), std::io::Error> {
        let mut event = EpollEvent {
            events: interest | EPOLLET,
            data: fd as u64,
        };
        let res = unsafe { epoll_ctl(self.epoll_fd, EPOLL_CTL_MOD, fd, &mut event) };
        if res < 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(())
    }

    pub fn poll(&self, timeout_ms: i32) -> Result<usize, std::io::Error> {
        const MAX_EVENTS: usize = 1024;
        let mut events: [EpollEvent; MAX_EVENTS] = unsafe { std::mem::zeroed() };

        let nfds = unsafe { epoll_wait(self.epoll_fd, events.as_mut_ptr(), MAX_EVENTS as i32, timeout_ms) };

        if nfds < 0 {
            let err = std::io::Error::last_os_error();
            if err.kind() == std::io::ErrorKind::Interrupted {
                return Ok(0);
            }
            return Err(err);
        }

        for i in 0..nfds as usize {
            let ev = &events[i];
            let fd = ev.data as RawFd;

            let cb_info = {
                if let Ok(map) = self.entries.lock() {
                    map.get(&fd).map(|e| (e.callback.as_ref() as *const (dyn Fn(u32, u64) + Send + Sync), e.generation))
                } else {
                    None
                }
            };

            if let Some((cb_ptr, gen)) = cb_info {
                unsafe { (*cb_ptr)(ev.events, gen); }
            }
        }

        Ok(nfds as usize)
    }
}

impl Drop for Reactor {
    fn drop(&mut self) {
        if self.epoll_fd >= 0 {
            unsafe { close(self.epoll_fd); }
        }
    }
}
