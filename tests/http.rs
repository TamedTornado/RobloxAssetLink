use std::{
    io::{BufRead, BufReader, Read, Write},
    net::TcpStream,
    process::{Child, Command, Stdio},
    time::Duration,
};

struct Process(Child);
impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn request(
    address: &str,
    method: &str,
    path: &str,
    token: &str,
    body: &str,
    origin: bool,
) -> (u16, serde_json::Value) {
    let mut stream = TcpStream::connect(address).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    write!(stream, "{method} {path} HTTP/1.1\r\nHost: {address}\r\nAuthorization: Bearer {token}\r\nContent-Length: {}\r\nConnection: close\r\n{}\r\n{body}", body.len(), if origin { "Origin: https://example.com\r\n" } else { "" }).unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    let (headers, data) = response.split_once("\r\n\r\n").unwrap();
    (
        headers.split_whitespace().nth(1).unwrap().parse().unwrap(),
        serde_json::from_str(data).unwrap(),
    )
}

#[test]
fn executable_serves_assets_and_updates_only_authorized_configuration() {
    let temporary = tempfile::tempdir().unwrap();
    let config = temporary.path().join("config.json");
    std::fs::write(&config, include_str!("../examples/building-kit.json")).unwrap();
    let child = Command::new(env!("CARGO_BIN_EXE_roblox-asset-link"))
        .args(["serve", "tests/fixtures", "--port", "0", "--config"])
        .arg(&config)
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut process = Process(child);
    let mut reader = BufReader::new(process.0.stderr.take().unwrap());
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    let address = line
        .trim()
        .strip_prefix("Asset Link: http://")
        .unwrap()
        .to_owned();
    line.clear();
    reader.read_line(&mut line).unwrap();
    let token = line
        .trim()
        .strip_prefix("Session token: ")
        .unwrap()
        .to_owned();
    assert_eq!(
        request(&address, "GET", "/snapshot", "wrong", "", false).0,
        403
    );
    assert_eq!(
        request(&address, "GET", "/snapshot", &token, "", true).0,
        403
    );
    assert_eq!(
        request(&address, "GET", "/../config", &token, "", false).0,
        404
    );
    let (status, snapshot) = request(&address, "GET", "/snapshot", &token, "", false);
    assert_eq!(status, 200);
    assert_eq!(snapshot["assets"].as_array().unwrap().len(), 3);
    let (_, mut update) = request(&address, "GET", "/config", &token, "", false);
    update.as_object_mut().unwrap().remove("text");
    update["value"]["pollSeconds"] = 9.into();
    let body = update.to_string();
    assert_eq!(
        request(&address, "PUT", "/config", &token, &body, false).0,
        200
    );
    assert_eq!(
        request(&address, "PUT", "/config", &token, &body, false).0,
        422
    );
    let disk: serde_json::Value = serde_json::from_slice(&std::fs::read(config).unwrap()).unwrap();
    assert_eq!(disk["pollSeconds"], 9.);
}
