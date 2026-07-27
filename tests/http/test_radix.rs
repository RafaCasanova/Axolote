
extern crate axolote;
use axolote::{Server, http::{HttpRequest, HttpResponse, HttpMethod}};
use std::process::Command;

fn main() {
    let mut server = Server::new("8089");
    server.add_route(HttpMethod::GET, "/api/users/{id:num}", |req: HttpRequest| {
        let id = req.params.get("id").unwrap();
        HttpResponse::ok(format!("user_{}", id))
    });
    server.add_route(HttpMethod::GET, "/api/users/{name:alpha}", |req: HttpRequest| {
        let name = req.params.get("name").unwrap();
        HttpResponse::ok(format!("name_{}", name))
    });
    
    std::thread::spawn(move || { server.run(); });
    std::thread::sleep(std::time::Duration::from_millis(100));

    let res2 = Command::new("curl").arg("-s").arg("http://127.0.0.1:8089/api/users/123").output().unwrap();
    println!("123: {}", String::from_utf8_lossy(&res2.stdout));

}
