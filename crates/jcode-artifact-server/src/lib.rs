use std::{net::SocketAddr, path::PathBuf};

use anyhow::{Context, Result};
use jcode_artifact_store::{Artifact, ArtifactStore, Revision};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

#[derive(Debug, Clone)]
pub struct ArtifactServer {
    store_root: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
    pub status: u16,
    pub content_type: &'static str,
    pub body: Vec<u8>,
}

#[derive(Debug, Clone)]
struct CatalogItem {
    artifact: Artifact,
    revisions: Vec<Revision>,
}

impl ArtifactServer {
    pub fn new(store_root: impl Into<PathBuf>) -> Self {
        Self {
            store_root: store_root.into(),
        }
    }

    pub async fn serve(self, addr: SocketAddr) -> Result<()> {
        let listener = TcpListener::bind(addr)
            .await
            .with_context(|| format!("binding artifact server to {addr}"))?;
        loop {
            let (stream, _) = listener.accept().await?;
            let server = self.clone();
            tokio::spawn(async move {
                let _ = server.handle_connection(stream).await;
            });
        }
    }

    async fn handle_connection(&self, mut stream: TcpStream) -> Result<()> {
        let mut buf = vec![0_u8; 8192];
        let n = stream.read(&mut buf).await?;
        let request = String::from_utf8_lossy(&buf[..n]);
        let Some(line) = request.lines().next() else {
            return Ok(());
        };
        let mut parts = line.split_whitespace();
        let method = parts.next().unwrap_or_default();
        let target = parts.next().unwrap_or("/");
        let response = if method == "GET" {
            self.route(target).await
        } else {
            Response::text(405, "method not allowed")
        };
        write_response(&mut stream, response).await
    }

    pub async fn route(&self, target: &str) -> Response {
        let path = target.split('?').next().unwrap_or(target);
        match path {
            "/" | "/catalog" => self.catalog_response().await,
            "/events/catalog" => self.catalog_sse_response().await,
            _ if path.starts_with("/artifacts/") => self.artifact_response(path).await,
            _ if path.starts_with("/raw/") => self.raw_response(path),
            _ => Response::text(404, "not found"),
        }
    }

    async fn catalog_response(&self) -> Response {
        match self.catalog_items().await {
            Ok(items) => Response::html(render_page("Artifact Catalog", &render_catalog(&items))),
            Err(err) => Response::text(500, &format!("catalog error: {err}")),
        }
    }

    async fn catalog_sse_response(&self) -> Response {
        match self.catalog_items().await {
            Ok(items) => {
                let fragment = render_catalog(&items);
                let body = format!(
                    "event: datastar-patch-elements\ndata: {}\n\n",
                    fragment.replace('\n', "")
                );
                Response {
                    status: 200,
                    content_type: "text/event-stream; charset=utf-8",
                    body: body.into_bytes(),
                }
            }
            Err(err) => Response::text(500, &format!("catalog error: {err}")),
        }
    }

    async fn artifact_response(&self, path: &str) -> Response {
        let parts: Vec<_> = path.trim_start_matches('/').split('/').collect();
        if parts.len() != 4 || parts[0] != "artifacts" || parts[2] != "revisions" {
            return Response::text(404, "not found");
        }
        let Ok(artifact_id) = safe_segment(parts[1]) else {
            return Response::text(400, "invalid artifact id");
        };
        let Ok(revision_id) = safe_segment(parts[3]) else {
            return Response::text(400, "invalid revision");
        };
        match self.revision_view(&artifact_id, &revision_id) {
            Ok(Some((artifact, revision))) => Response::html(render_page(
                &format!("{} · r{}", artifact.title, revision.number),
                &render_revision(&artifact, &revision),
            )),
            Ok(None) => Response::text(404, "artifact revision not found"),
            Err(err) => Response::text(500, &format!("artifact revision error: {err}")),
        }
    }

    fn raw_response(&self, path: &str) -> Response {
        let parts: Vec<_> = path.trim_start_matches('/').split('/').collect();
        if parts.len() != 3 || parts[0] != "raw" {
            return Response::text(404, "not found");
        }
        let Ok(revision_id) = safe_segment(parts[1]) else {
            return Response::text(400, "invalid asset path");
        };
        let Ok(kind) = safe_segment(parts[2]) else {
            return Response::text(400, "invalid asset path");
        };
        let store = match self.open_store() {
            Ok(store) => store,
            Err(err) => return Response::text(500, &format!("asset error: {err}")),
        };
        let revision = match store.get_revision(&revision_id) {
            Ok(Some(revision)) => revision,
            Ok(None) => return Response::text(404, "asset not found"),
            Err(err) => return Response::text(500, &format!("asset error: {err}")),
        };
        let result = match kind.as_str() {
            "source" => store.read_source_bytes(&revision),
            "rendered" => store.read_rendered_bytes(&revision),
            _ => return Response::text(404, "asset not found"),
        };
        match result {
            Ok(bytes) => Response {
                status: 200,
                content_type: if kind == "rendered" {
                    "text/html; charset=utf-8"
                } else {
                    "text/plain; charset=utf-8"
                },
                body: bytes,
            },
            Err(_) => Response::text(404, "asset not found"),
        }
    }

    async fn catalog_items(&self) -> Result<Vec<CatalogItem>> {
        let store = self.open_store()?;
        let mut items = Vec::new();
        for artifact in store.list_artifacts()? {
            let revisions = store.list_revisions(&artifact.id)?;
            items.push(CatalogItem {
                artifact,
                revisions,
            });
        }
        Ok(items)
    }

    fn revision_view(
        &self,
        artifact_id: &str,
        revision_id: &str,
    ) -> Result<Option<(Artifact, Revision)>> {
        let store = self.open_store()?;
        let Some(artifact) = store.get_artifact(artifact_id)? else {
            return Ok(None);
        };
        let Some(revision) = store.get_revision(revision_id)? else {
            return Ok(None);
        };
        if revision.artifact_id != artifact.id {
            return Ok(None);
        }
        Ok(Some((artifact, revision)))
    }

    fn open_store(&self) -> Result<ArtifactStore> {
        ArtifactStore::open_migrate(self.database_path(), self.asset_root()).map_err(Into::into)
    }

    fn database_path(&self) -> PathBuf {
        self.store_root.join("artifacts.sqlite3")
    }

    fn asset_root(&self) -> PathBuf {
        self.store_root.join("assets")
    }
}

impl Response {
    fn html(body: String) -> Self {
        Self {
            status: 200,
            content_type: "text/html; charset=utf-8",
            body: body.into_bytes(),
        }
    }

    fn text(status: u16, body: &str) -> Self {
        Self {
            status,
            content_type: "text/plain; charset=utf-8",
            body: body.as_bytes().to_vec(),
        }
    }
}

async fn write_response(stream: &mut TcpStream, response: Response) -> Result<()> {
    let status_text = match response.status {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        405 => "Method Not Allowed",
        500 => "Internal Server Error",
        _ => "OK",
    };
    let headers = format!(
        "HTTP/1.1 {} {}\r\ncontent-type: {}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
        response.status,
        status_text,
        response.content_type,
        response.body.len()
    );
    stream.write_all(headers.as_bytes()).await?;
    stream.write_all(&response.body).await?;
    Ok(())
}

fn safe_segment(segment: &str) -> Result<String, ()> {
    if segment.is_empty()
        || segment == "."
        || segment == ".."
        || segment.contains('/')
        || segment.contains('\\')
        || segment.contains('%')
    {
        return Err(());
    }
    Ok(segment.to_string())
}

fn render_page(title: &str, main: &str) -> String {
    format!(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{}</title>
<style>
:root {{ color-scheme: light; --ink:#190b05; --paper:#f4efe6; --red:#c2291f; --line:#190b05; --wash:#f9d9cf; }}
* {{ box-sizing: border-box; }}
body {{ margin:0; background:var(--paper); color:var(--ink); font:16px/1.5 ui-monospace,SFMono-Regular,Menlo,Consolas,monospace; }}
a {{ color:inherit; text-decoration-thickness: .16em; text-underline-offset: .18em; }}
a:focus-visible {{ outline:4px solid var(--red); outline-offset:3px; }}
header {{ border-bottom:6px solid var(--line); background:var(--red); color:var(--paper); padding:1.25rem clamp(1rem,4vw,3rem); }}
h1 {{ margin:0; font-size:clamp(2rem,7vw,5rem); line-height:.92; letter-spacing:-.06em; text-transform:uppercase; }}
main {{ padding:clamp(1rem,4vw,3rem); }}
.panel {{ border:4px solid var(--line); background:#fffaf1; box-shadow:10px 10px 0 var(--line); padding:1rem; max-width:72rem; }}
ul {{ list-style:none; padding:0; margin:0; display:grid; gap:.75rem; }}
li {{ border:3px solid var(--line); background:var(--wash); padding:.75rem; }}
.meta {{ font-size:.85rem; text-transform:uppercase; letter-spacing:.08em; }}
.empty {{ border:3px dashed var(--line); padding:1rem; }}
</style>
</head>
<body><header><h1>{}</h1></header><main>{}</main></body>
</html>"#,
        escape_html(title),
        escape_html(title),
        main
    )
}

fn render_catalog(items: &[CatalogItem]) -> String {
    if items.is_empty() {
        return "<section id=\"catalog\" class=\"panel\"><p class=\"empty\">No artifacts found.</p></section>".to_string();
    }
    let mut out = String::from(
        "<section id=\"catalog\" class=\"panel\" aria-label=\"Artifact catalog\"><ul>",
    );
    for item in items {
        out.push_str("<li>");
        out.push_str(&format!(
            "<div class=\"meta\">artifact</div><strong>{}</strong>",
            escape_html(&item.artifact.title)
        ));
        if item.revisions.is_empty() {
            out.push_str("<p>No revisions.</p>");
        } else {
            out.push_str("<ul>");
            for revision in &item.revisions {
                out.push_str(&format!(
                    "<li><a href=\"/artifacts/{}/revisions/{}\">revision {}</a></li>",
                    escape_attr(&item.artifact.id),
                    escape_attr(&revision.id),
                    revision.number
                ));
            }
            out.push_str("</ul>");
        }
        out.push_str("</li>");
    }
    out.push_str("</ul></section>");
    out
}

fn render_revision(artifact: &Artifact, revision: &Revision) -> String {
    format!(
        "<section class=\"panel\" aria-label=\"Artifact revision\"><p class=\"meta\">artifact {} · revision {}</p><ul><li><a href=\"/raw/{}/source\">Source asset</a></li><li><a href=\"/raw/{}/rendered\">Rendered asset</a></li></ul><p><a href=\"/catalog\">Back to catalog</a></p></section>",
        escape_html(&artifact.title),
        revision.number,
        escape_attr(&revision.id),
        escape_attr(&revision.id)
    )
}

fn escape_html(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn escape_attr(input: &str) -> String {
    escape_html(input)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seed_store(root: &PathBuf) -> (Artifact, Revision) {
        let store =
            ArtifactStore::open_migrate(root.join("artifacts.sqlite3"), root.join("assets"))
                .expect("store opens");
        let artifact = store
            .create_artifact("deck-alpha", "Deck Alpha")
            .expect("artifact created");
        let revision = store
            .add_revision(&artifact.id, b"# Source", b"<h1>Rendered</h1>")
            .expect("revision created");
        (artifact, revision)
    }

    #[tokio::test]
    async fn catalog_route_renders_artifacts_and_revisions() {
        let temp = tempfile::tempdir().unwrap();
        let (artifact, revision) = seed_store(&temp.path().to_path_buf());

        let server = ArtifactServer::new(temp.path());
        let response = server.route("/catalog").await;
        let body = String::from_utf8(response.body).unwrap();

        assert_eq!(response.status, 200);
        assert!(body.contains("Artifact Catalog"));
        assert!(body.contains("Deck Alpha"));
        assert!(body.contains(&format!(
            "/artifacts/{}/revisions/{}",
            artifact.id, revision.id
        )));
        assert!(!body.contains("store_root"));
        assert!(!body.contains("artifacts.sqlite3"));
    }

    #[tokio::test]
    async fn revision_route_renders_raw_asset_links() {
        let temp = tempfile::tempdir().unwrap();
        let (artifact, revision) = seed_store(&temp.path().to_path_buf());

        let server = ArtifactServer::new(temp.path());
        let response = server
            .route(&format!(
                "/artifacts/{}/revisions/{}",
                artifact.id, revision.id
            ))
            .await;
        let body = String::from_utf8(response.body).unwrap();

        assert_eq!(response.status, 200);
        assert!(body.contains("Artifact revision"));
        assert!(body.contains(&format!("/raw/{}/source", revision.id)));
        assert!(body.contains(&format!("/raw/{}/rendered", revision.id)));
    }

    #[tokio::test]
    async fn raw_route_serves_store_assets_and_rejects_path_traversal() {
        let temp = tempfile::tempdir().unwrap();
        let (_, revision) = seed_store(&temp.path().to_path_buf());
        let server = ArtifactServer::new(temp.path());

        let source = server.route(&format!("/raw/{}/source", revision.id)).await;
        assert_eq!(source.status, 200);
        assert_eq!(String::from_utf8(source.body).unwrap(), "# Source");

        let traversal = server.route("/raw/../Cargo.toml").await;
        assert_eq!(traversal.status, 400);
        assert_eq!(
            String::from_utf8(traversal.body).unwrap(),
            "invalid asset path"
        );
    }

    #[tokio::test]
    async fn artifact_route_rejects_encoded_or_dotdot_segments() {
        let temp = tempfile::tempdir().unwrap();
        let server = ArtifactServer::new(temp.path());

        assert_eq!(server.route("/artifacts/../revisions/r1").await.status, 400);
        assert_eq!(
            server.route("/artifacts/%2e%2e/revisions/r1").await.status,
            400
        );
    }

    #[tokio::test]
    async fn sse_route_returns_datastar_patch_event() {
        let temp = tempfile::tempdir().unwrap();
        seed_store(&temp.path().to_path_buf());
        let server = ArtifactServer::new(temp.path());

        let response = server.route("/events/catalog").await;
        let body = String::from_utf8(response.body).unwrap();

        assert_eq!(response.status, 200);
        assert_eq!(response.content_type, "text/event-stream; charset=utf-8");
        assert!(body.contains("event: datastar-patch-elements"));
        assert!(body.contains("Deck Alpha"));
    }
}
