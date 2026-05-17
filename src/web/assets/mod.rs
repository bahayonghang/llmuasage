use axum::{
    http::header,
    response::{IntoResponse, Response},
};

pub(crate) struct WebAsset {
    pub(crate) path: &'static str,
    pub(crate) content_type: &'static str,
    pub(crate) body: &'static str,
}

impl WebAsset {
    pub(crate) fn as_response(&self) -> Response {
        ([(header::CONTENT_TYPE, self.content_type)], self.body).into_response()
    }
}

pub(crate) fn asset_manifest() -> &'static [WebAsset] {
    ASSET_MANIFEST
}

pub(crate) fn find_asset(path: &str) -> Option<&'static WebAsset> {
    ASSET_MANIFEST.iter().find(|asset| asset.path == path)
}

const ASSET_MANIFEST: &[WebAsset] = &[
    WebAsset {
        path: "base.css",
        content_type: "text/css; charset=utf-8",
        body: include_str!("base.css"),
    },
    WebAsset {
        path: "layout.css",
        content_type: "text/css; charset=utf-8",
        body: include_str!("layout.css"),
    },
    WebAsset {
        path: "components.css",
        content_type: "text/css; charset=utf-8",
        body: include_str!("components.css"),
    },
    WebAsset {
        path: "charts.css",
        content_type: "text/css; charset=utf-8",
        body: include_str!("charts.css"),
    },
    WebAsset {
        path: "app.js",
        content_type: "application/javascript; charset=utf-8",
        body: include_str!("app.js"),
    },
    WebAsset {
        path: "copy.js",
        content_type: "application/javascript; charset=utf-8",
        body: include_str!("copy.js"),
    },
    WebAsset {
        path: "i18n.js",
        content_type: "application/javascript; charset=utf-8",
        body: include_str!("i18n.js"),
    },
    WebAsset {
        path: "theme.js",
        content_type: "application/javascript; charset=utf-8",
        body: include_str!("theme.js"),
    },
    WebAsset {
        path: "runtime.js",
        content_type: "application/javascript; charset=utf-8",
        body: include_str!("runtime.js"),
    },
    WebAsset {
        path: "data.js",
        content_type: "application/javascript; charset=utf-8",
        body: include_str!("data.js"),
    },
    WebAsset {
        path: "data/fetch.js",
        content_type: "application/javascript; charset=utf-8",
        body: include_str!("data/fetch.js"),
    },
    WebAsset {
        path: "data/format.js",
        content_type: "application/javascript; charset=utf-8",
        body: include_str!("data/format.js"),
    },
    WebAsset {
        path: "data/derive.js",
        content_type: "application/javascript; charset=utf-8",
        body: include_str!("data/derive.js"),
    },
    WebAsset {
        path: "render.js",
        content_type: "application/javascript; charset=utf-8",
        body: include_str!("render.js"),
    },
    WebAsset {
        path: "render/hero.js",
        content_type: "application/javascript; charset=utf-8",
        body: include_str!("render/hero.js"),
    },
    WebAsset {
        path: "render/trends.js",
        content_type: "application/javascript; charset=utf-8",
        body: include_str!("render/trends.js"),
    },
    WebAsset {
        path: "render/models.js",
        content_type: "application/javascript; charset=utf-8",
        body: include_str!("render/models.js"),
    },
    WebAsset {
        path: "render/sources.js",
        content_type: "application/javascript; charset=utf-8",
        body: include_str!("render/sources.js"),
    },
    WebAsset {
        path: "render/projects.js",
        content_type: "application/javascript; charset=utf-8",
        body: include_str!("render/projects.js"),
    },
    WebAsset {
        path: "render/behavior.js",
        content_type: "application/javascript; charset=utf-8",
        body: include_str!("render/behavior.js"),
    },
    WebAsset {
        path: "render/costs.js",
        content_type: "application/javascript; charset=utf-8",
        body: include_str!("render/costs.js"),
    },
    WebAsset {
        path: "render/insights.js",
        content_type: "application/javascript; charset=utf-8",
        body: include_str!("render/insights.js"),
    },
    WebAsset {
        path: "render/charts.js",
        content_type: "application/javascript; charset=utf-8",
        body: include_str!("render/charts.js"),
    },
    WebAsset {
        path: "render/tables.js",
        content_type: "application/javascript; charset=utf-8",
        body: include_str!("render/tables.js"),
    },
    WebAsset {
        path: "render/health.js",
        content_type: "application/javascript; charset=utf-8",
        body: include_str!("render/health.js"),
    },
    WebAsset {
        path: "favicon.svg",
        content_type: "image/svg+xml",
        body: include_str!("favicon.svg"),
    },
];
