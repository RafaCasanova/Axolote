
extern crate axolote;
use axolote::{Server, http::{HttpRequest, HttpResponse}};

fn main() {
    let mut server = Server::new("8081");
    server.add_route(axolote::http::HttpMethod::GET, "/ping", |req: HttpRequest| {
        HttpResponse::ok("pong")
    });
    std::thread::spawn(move || { server.run(); });
    std::thread::sleep(std::time::Duration::from_millis(100));
    let resp = std::process::Command::new("curl")
        .arg("-s")
        .arg("http://127.0.0.1:8081/ping")
        .output()
        .expect("failed to execute curl");
    println!("Response: {}", String::from_utf8_lossy(&resp.stdout));
}
