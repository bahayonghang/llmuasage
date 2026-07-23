use std::sync::OnceLock;

use axum::{
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};

pub(crate) struct WebAsset {
    pub(crate) path: &'static str,
    pub(crate) content_type: &'static str,
    pub(crate) body: &'static str,
    /// 首次响应时按内容计算的 ETag（FNV-1a 64-bit 十六进制），之后复用。
    etag: OnceLock<String>,
}

impl WebAsset {
    fn etag(&self) -> &str {
        self.etag.get_or_init(|| {
            let mut hash = 0xcbf29ce484222325_u64;
            for &byte in self.body.as_bytes() {
                hash ^= u64::from(byte);
                hash = hash.wrapping_mul(0x100000001b3);
            }
            format!("\"{hash:016x}\"")
        })
    }

    pub(crate) fn as_response(&self, headers: &HeaderMap) -> Response {
        let etag = self.etag();
        if if_none_match_matches(headers, etag) {
            return (
                StatusCode::NOT_MODIFIED,
                [(header::CACHE_CONTROL, "no-cache"), (header::ETAG, etag)],
            )
                .into_response();
        }
        (
            [
                (header::CONTENT_TYPE, self.content_type),
                (header::CACHE_CONTROL, "no-cache"),
                (header::ETAG, etag),
            ],
            self.body,
        )
            .into_response()
    }
}

fn if_none_match_matches(headers: &HeaderMap, etag: &str) -> bool {
    let Some(value) = headers.get(header::IF_NONE_MATCH) else {
        return false;
    };
    let Ok(value) = value.to_str() else {
        return false;
    };
    value.split(',').any(|candidate| {
        let candidate = candidate.trim();
        candidate == "*" || candidate.strip_prefix("W/").unwrap_or(candidate) == etag
    })
}

pub(crate) fn asset_manifest() -> &'static [WebAsset] {
    &ASSET_MANIFEST
}

pub(crate) fn find_asset(path: &str) -> Option<&'static WebAsset> {
    ASSET_MANIFEST.iter().find(|asset| asset.path == path)
}

static ASSET_MANIFEST: [WebAsset; 26] = [
    WebAsset {
        path: "base.css",
        content_type: "text/css; charset=utf-8",
        body: include_str!("base.css"),
        etag: OnceLock::new(),
    },
    WebAsset {
        path: "layout.css",
        content_type: "text/css; charset=utf-8",
        body: include_str!("layout.css"),
        etag: OnceLock::new(),
    },
    WebAsset {
        path: "components.css",
        content_type: "text/css; charset=utf-8",
        body: include_str!("components.css"),
        etag: OnceLock::new(),
    },
    WebAsset {
        path: "charts.css",
        content_type: "text/css; charset=utf-8",
        body: include_str!("charts.css"),
        etag: OnceLock::new(),
    },
    WebAsset {
        path: "app.js",
        content_type: "application/javascript; charset=utf-8",
        body: include_str!("app.js"),
        etag: OnceLock::new(),
    },
    WebAsset {
        path: "load-state.js",
        content_type: "application/javascript; charset=utf-8",
        body: include_str!("load-state.js"),
        etag: OnceLock::new(),
    },
    WebAsset {
        path: "copy.js",
        content_type: "application/javascript; charset=utf-8",
        body: include_str!("copy.js"),
        etag: OnceLock::new(),
    },
    WebAsset {
        path: "i18n.js",
        content_type: "application/javascript; charset=utf-8",
        body: include_str!("i18n.js"),
        etag: OnceLock::new(),
    },
    WebAsset {
        path: "theme.js",
        content_type: "application/javascript; charset=utf-8",
        body: include_str!("theme.js"),
        etag: OnceLock::new(),
    },
    WebAsset {
        path: "runtime.js",
        content_type: "application/javascript; charset=utf-8",
        body: include_str!("runtime.js"),
        etag: OnceLock::new(),
    },
    WebAsset {
        path: "data.js",
        content_type: "application/javascript; charset=utf-8",
        body: include_str!("data.js"),
        etag: OnceLock::new(),
    },
    WebAsset {
        path: "data/fetch.js",
        content_type: "application/javascript; charset=utf-8",
        body: include_str!("data/fetch.js"),
        etag: OnceLock::new(),
    },
    WebAsset {
        path: "data/format.js",
        content_type: "application/javascript; charset=utf-8",
        body: include_str!("data/format.js"),
        etag: OnceLock::new(),
    },
    WebAsset {
        path: "data/derive.js",
        content_type: "application/javascript; charset=utf-8",
        body: include_str!("data/derive.js"),
        etag: OnceLock::new(),
    },
    WebAsset {
        path: "data/render-key.js",
        content_type: "application/javascript; charset=utf-8",
        body: include_str!("data/render-key.js"),
        etag: OnceLock::new(),
    },
    WebAsset {
        path: "render/hero.js",
        content_type: "application/javascript; charset=utf-8",
        body: include_str!("render/hero.js"),
        etag: OnceLock::new(),
    },
    WebAsset {
        path: "render/sync-command-center.js",
        content_type: "application/javascript; charset=utf-8",
        body: include_str!("render/sync-command-center.js"),
        etag: OnceLock::new(),
    },
    WebAsset {
        path: "render/trends.js",
        content_type: "application/javascript; charset=utf-8",
        body: include_str!("render/trends.js"),
        etag: OnceLock::new(),
    },
    WebAsset {
        path: "render/models.js",
        content_type: "application/javascript; charset=utf-8",
        body: include_str!("render/models.js"),
        etag: OnceLock::new(),
    },
    WebAsset {
        path: "render/sources.js",
        content_type: "application/javascript; charset=utf-8",
        body: include_str!("render/sources.js"),
        etag: OnceLock::new(),
    },
    WebAsset {
        path: "render/projects.js",
        content_type: "application/javascript; charset=utf-8",
        body: include_str!("render/projects.js"),
        etag: OnceLock::new(),
    },
    WebAsset {
        path: "render/behavior.js",
        content_type: "application/javascript; charset=utf-8",
        body: include_str!("render/behavior.js"),
        etag: OnceLock::new(),
    },
    WebAsset {
        path: "render/explorer.js",
        content_type: "application/javascript; charset=utf-8",
        body: include_str!("render/explorer.js"),
        etag: OnceLock::new(),
    },
    WebAsset {
        path: "render/costs.js",
        content_type: "application/javascript; charset=utf-8",
        body: include_str!("render/costs.js"),
        etag: OnceLock::new(),
    },
    WebAsset {
        path: "render/insights.js",
        content_type: "application/javascript; charset=utf-8",
        body: include_str!("render/insights.js"),
        etag: OnceLock::new(),
    },
    WebAsset {
        path: "favicon.svg",
        content_type: "image/svg+xml",
        body: include_str!("favicon.svg"),
        etag: OnceLock::new(),
    },
];
