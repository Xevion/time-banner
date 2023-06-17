mod config;

use std::net::SocketAddr;
use std::time::SystemTime;

use axum::{http::StatusCode, response::IntoResponse, Router, routing::{get}};
use axum::body::{Full};
use axum::extract::{ConnectInfo, Path};
use axum::http::{header, HeaderMap};
use axum::response::Response;
use dotenvy::dotenv;
use lazy_static::lazy_static;
use config::Configuration;
use tera::{Tera, Context};
use timeago::Formatter;

mod svg;

lazy_static! {
    pub static ref TEMPLATES: Tera = {
        let mut _tera = match Tera::new("templates/**/*.svg") {
            Ok(t) => {
                let names: Vec<&str> = t.get_template_names().collect();
                println!("{} templates found ([{}]).", names.len(), names.join(", "));
                t
            },
            Err(e) => {
                println!("Parsing error(s): {}", e);
                ::std::process::exit(1);
            }
        };
        _tera
    };
}


#[tokio::main]
async fn main() {
    // Parse dotenv files and expose them as environment variables
    dotenv().ok();

    // envy uses our Configuration struct to parse environment variables
    let config = envy::from_env::<Configuration>().expect("Please provide PORT env var");

    // initialize tracing
    tracing_subscriber::fmt()
        // With the log_level from our config
        .with_max_level(config.log_level())
        .init();

    let app = Router::new().route("/:path", get(root_handler));
    let addr = SocketAddr::from((config.socket_addr(), config.port));
    axum::Server::bind(&addr)
        .serve(app.into_make_service_with_connect_info::<SocketAddr>())
        .await
        .unwrap();
}

fn convert_epoch(epoch: u64) -> SystemTime {
    SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(epoch)
}

fn parse_path(path: String) -> Result<(SystemTime, String), String> {
    if path.contains(".") {
        let split_path = path.split_once(".").unwrap();
        let epoch_int = split_path.0.parse::<u64>();
        if epoch_int.is_err() {
            return Err("Epoch is not a valid integer.".to_string());
        }

        return Ok((convert_epoch(epoch_int.unwrap()), split_path.1.parse().unwrap()));
    }

    let epoch_int = path.parse::<u64>();
    if epoch_int.is_err() {
        return Err("Epoch is not a valid integer.".to_string());
    }

    Ok(
        (convert_epoch(epoch_int.unwrap()), String::from("svg"))
    )
}

// basic handler that responds with a static string
async fn root_handler(Path(path): Path<String>) -> impl IntoResponse {
    let renderer = svg::Renderer::new();
    let mut context = Context::new();
    let f = Formatter::new();

    let parse_result = parse_path(path);
    if parse_result.is_err() {
        return Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body(Full::from(parse_result.err().unwrap()))
            .unwrap();
    }
    let (epoch, extension) = parse_result.unwrap();

    context.insert("text", &f.convert(epoch.elapsed().ok().unwrap()));
    context.insert("width", "512");
    context.insert("height", "34");

    let data = TEMPLATES.render("basic.svg", &context);
    if data.is_err() {
        return Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(Full::from(
                format!("Template Could Not Be Rendered :: {}", data.err().unwrap())
            ))
            .unwrap();
    }

    match extension.as_str() {
        "svg" => {
            Response::builder()
                .header(header::CONTENT_TYPE, "image/svg+xml")
                .body(Full::from(data.unwrap()))
                .unwrap()
        }
        "png" => {
            let raw_image = renderer.render(data.unwrap().into_bytes());
            if raw_image.is_err() {
                return Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .body(Full::from(
                        format!("Internal Server Error :: {}", raw_image.err().unwrap())
                    ))
                    .unwrap();
            }

            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, "image/x-png")
                .body(Full::from(raw_image.unwrap()))
                .unwrap()
        }
        _ => {
            Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body(Full::from("Unsupported extension."))
                .unwrap()
        }
    }
}