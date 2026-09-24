use clap::Parser;
use roblox_asset_link::{Result, catalog, config};
use serde::Deserialize;
use std::path::{Path, PathBuf};
use tiny_http::{Header, Method, Response, Server};

#[derive(Parser)]
#[command(about = "Studio adapter server; asset management belongs in the roblox CLI")]
struct Cli {
    #[arg(long)]
    catalog: PathBuf,
    #[arg(long, default_value_t = 34873)]
    port: u16,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ConfigUpdate {
    revision: String,
    value: config::Config,
}

fn config_response(path: &Path) -> Result<String> {
    let catalog = catalog::read(path)?;
    let revision = catalog.revision()?;
    let value = catalog.config;
    Ok(serde_json::json!({"revision": revision, "value": value,
        "text": serde_json::to_string_pretty(&value)?})
    .to_string())
}

fn update_config(path: &Path, body: &str) -> Result<String> {
    let update: ConfigUpdate = serde_json::from_str(body)?;
    catalog::mutate(path, Some(&update.revision), |current| {
        if current.config.project_id != update.value.project_id {
            return Err("projectId is immutable; initialize another catalog".into());
        }
        current.config = update.value;
        current.snapshot(path)?;
        Ok(())
    })?;
    config_response(path)
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let path = cli.catalog.canonicalize()?;
    catalog::read(&path)?;
    let port = cli.port;
    let server = Server::http((std::net::Ipv4Addr::LOCALHOST, port))?;
    let token = uuid::Uuid::new_v4().to_string();
    eprintln!(
        "Studio adapter: http://{}\nSession token: {token}\nCatalog: {}",
        server.server_addr(),
        path.display()
    );

    for mut request in server.incoming_requests() {
        let authorized = request.headers().iter().any(|h| {
            h.field.equiv("Authorization") && h.value.as_str() == format!("Bearer {token}")
        });
        let has_origin = request.headers().iter().any(|h| h.field.equiv("Origin"));
        let result: Result<String>;
        let status;
        if !authorized || has_origin {
            status = 403;
            result = Err("forbidden".into());
        } else if request.method() == &Method::Get && request.url() == "/snapshot" {
            status = 200;
            result =
                catalog::read(&path).and_then(|c| Ok(serde_json::to_string(&c.snapshot(&path)?)?));
        } else if request.method() == &Method::Get && request.url() == "/config" {
            status = 200;
            result = config_response(&path);
        } else if request.method() == &Method::Put && request.url() == "/config" {
            status = 200;
            let mut body = String::new();
            result = request
                .as_reader()
                .read_to_string(&mut body)
                .map_err(Into::into)
                .and_then(|_| update_config(&path, &body));
        } else {
            status = 404;
            result = Err("not found".into());
        }
        let (status, body) = match result {
            Ok(body) => (status, body),
            Err(error) => {
                eprintln!("Request failed: {error}");
                (
                    if status == 200 { 422 } else { status },
                    serde_json::json!({"error":error.to_string()}).to_string(),
                )
            }
        };
        let response = Response::from_string(body)
            .with_status_code(status)
            .with_header(Header::from_bytes("Content-Type", "application/json").unwrap())
            .with_header(Header::from_bytes("Cache-Control", "no-store").unwrap());
        if let Err(error) = request.respond(response) {
            eprintln!("Response failed: {error}");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ui_update_persists_and_stale_update_cannot_overwrite_it() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.json");
        let value: config::Config =
            serde_json::from_str(include_str!("../examples/building-kit.json")).unwrap();
        catalog::initialize(&path, value).unwrap();
        let loaded = catalog::read(&path).unwrap();
        let revision = loaded.revision().unwrap();
        let mut value = loaded.config;
        value.poll_seconds = 7.;
        let body = serde_json::json!({"revision":revision,"value":value}).to_string();
        update_config(&path, &body).unwrap();
        assert_eq!(catalog::read(&path).unwrap().config.poll_seconds, 7.);
        assert!(update_config(&path, &body).is_err());
        assert_eq!(catalog::read(&path).unwrap().config.poll_seconds, 7.);
    }
}
