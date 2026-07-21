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

pub type EventCallback = Box<dyn Fn(u32) + Send + Sync + 'static>;

pub struct Reactor {
    epoll_fd: RawFd,
    callbacks: Arc<Mutex<HashMap<RawFd, EventCallback>>>,
}

impl Reactor {
    pub fn new() -> Result<Self, std::io::Error> {
        let epoll_fd = unsafe { epoll_create1(0) };
        if epoll_fd < 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(Self {
            epoll_fd,
            callbacks: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    pub fn register<F>(&self, fd: RawFd, interest: u32, callback: F) -> Result<(), std::io::Error>
    where
        F: Fn(u32) + Send + Sync + 'static,
    {
        let mut event = EpollEvent {
            events: interest | EPOLLET,
            data: fd as u64,
        };

        let res = unsafe { epoll_ctl(self.epoll_fd, EPOLL_CTL_ADD, fd, &mut event) };
        if res < 0 {
            return Err(std::io::Error::last_os_error());
        }

        if let Ok(mut cb_map) = self.callbacks.lock() {
            cb_map.insert(fd, Box::new(callback));
        }

        Ok(())
    }

    pub fn unregister(&self, fd: RawFd) -> Result<(), std::io::Error> {
        let mut event = EpollEvent {
            events: 0,
            data: fd as u64,
        };
        unsafe { epoll_ctl(self.epoll_fd, EPOLL_CTL_DEL, fd, &mut event) };
        if let Ok(mut cb_map) = self.callbacks.lock() {
            cb_map.remove(&fd);
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

            let callback_opt = {
                if let Ok(cb_map) = self.callbacks.lock() {
                    cb_map.get(&fd).map(|cb| cb.as_ref() as *const (dyn Fn(u32) + Send + Sync))
                } else {
                    None
                }
            };

            if let Some(cb_ptr) = callback_opt {
                unsafe { (*cb_ptr)(ev.events); }
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
