#!/usr/bin/env python3
"""A tiny mock of the Silo v2 API used by the ignored `test_v2_against_mock`
test. It mirrors the shapes in silo-server/contracts/api/v2/fixtures so the
source's v2 code paths (envelopes, string file ids, `seek` pagination) can be
exercised without a running v2 server.

Usage:
    python3 tests/mock_silo_v2.py [port]
    cargo test -- --ignored
"""
import io
import json
import sys
import zipfile
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import urlparse, parse_qs

try:
    from PIL import Image
except Exception:  # pragma: no cover
    Image = None

PORT = int(sys.argv[1]) if len(sys.argv) > 1 else 8799


def page_png(color):
    if Image is None:
        # 1x1 PNG fallback.
        return bytes.fromhex(
            "89504e470d0a1a0a0000000d4948445200000001000000010806000000"
            "1f15c4890000000d49444154789c6360606060000000050001a5f64540"
            "0000000049454e44ae426082"
        )
    image = Image.new("RGB", (32, 48), color)
    buffer = io.BytesIO()
    image.save(buffer, format="PNG")
    return buffer.getvalue()


def build_cbz():
    buffer = io.BytesIO()
    with zipfile.ZipFile(buffer, "w", zipfile.ZIP_DEFLATED) as archive:
        archive.writestr("page2.png", page_png((0, 128, 255)))
        archive.writestr("page1.png", page_png((255, 64, 64)))
    return buffer.getvalue()


CBZ = build_cbz()

LIBRARIES = {
    "items": [
        {"id": "12", "name": "Manga", "type": "manga", "sort_order": 1},
        {"id": "7", "name": "Movies", "type": "movies", "sort_order": 2},
    ]
}

CATALOG = {
    "page": {"next_cursor": "cursor-1", "has_more": True},
    "items": [
        {
            "content_id": "m1",
            "type": "manga",
            "title": "Mock Manga One",
            "genres": ["Action"],
            "keywords": [],
            "status": "matched",
            "poster_url": "http://example.test/m1.jpg",
        },
        {
            "content_id": "m2",
            "type": "manga",
            "title": "Mock Manga Two",
            "genres": [],
            "keywords": [],
            "status": "matched",
        },
    ],
    "total": 3,
    "total_exact": True,
}

CATALOG_SECOND = {
    "page": {"next_cursor": None, "has_more": False},
    "items": [
        {
            "content_id": "m3",
            "type": "manga",
            "title": "Mock Manga Three",
            "genres": [],
            "keywords": [],
            "status": "matched",
        }
    ],
    "total": 3,
    "total_exact": True,
}

DETAIL = {
    "content_id": "m1",
    "type": "manga",
    "title": "Mock Manga One",
    "overview": "A mock manga.",
    "genres": ["Action", "Adventure"],
    "keywords": [],
    "show_status": "Ongoing",
    "poster_url": "http://example.test/m1.jpg",
    "backdrop_url": "http://example.test/m1-b.jpg",
    "crew": [{"name": "Mock Author", "job": "Author"}],
    "cast": [],
    "versions": [],
    "manga": {
        "chapters": [
            {"content_id": "c1", "title": "Chapter 1", "chapter_index": 1, "read": False},
            {"content_id": "c2", "title": "Chapter 2", "chapter_index": 2, "read": False},
        ]
    },
}

CHAPTER = {
    "content_id": "c1",
    "type": "ebook",
    "title": "Chapter 1",
    "versions": [
        {
            "file_id": "55",
            "file_name": "c1.cbz",
            "container": "cbz",
            "file_size": len(CBZ),
            "duration": 2,
        }
    ],
    "series_id": "m1",
    "series_title": "Mock Manga One",
}


class Handler(BaseHTTPRequestHandler):
    def log_message(self, *args):
        pass

    def _json(self, payload, status=200):
        body = json.dumps(payload).encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def _bytes(self, payload):
        self.send_response(200)
        self.send_header("Content-Type", "application/vnd.comicbook+zip")
        self.send_header("Content-Length", str(len(payload)))
        self.end_headers()
        self.wfile.write(payload)

    def do_GET(self):
        parsed = urlparse(self.path)
        path = parsed.path
        if path == "/api/v2/system/info":
            return self._json({"server_version": "mock", "api_major": 2})
        if path == "/api/v2/profiles":
            return self._json(
                {"items": [{"id": "p1", "name": "Main", "has_pin": False, "is_primary": True}]}
            )
        if path == "/api/v2/user/libraries":
            return self._json(LIBRARIES)
        if path == "/api/v2/catalog":
            query = parse_qs(parsed.query)
            seek = int(query.get("seek", ["0"])[0])
            return self._json(CATALOG_SECOND if seek > 0 else CATALOG)
        if path == "/api/v2/catalog/filters":
            return self._json({"genres": ["Action", "Adventure"], "authors": ["Mock Author"]})
        if path == "/api/v2/library/12/sections":
            return self._json(
                {
                    "sections": [
                        {
                            "id": "recent",
                            "section_type": "recently_added",
                            "title": "Recently Added",
                            "items": CATALOG["items"],
                        }
                    ]
                }
            )
        if path.startswith("/api/v2/catalog/items/"):
            content_id = path.rsplit("/", 1)[-1]
            if content_id == "c1":
                return self._json(CHAPTER)
            return self._json(DETAIL)
        if path.startswith("/api/v2/ebooks/") and path.endswith("/read"):
            return self._bytes(CBZ)
        return self._json({"type": "about:blank", "title": "not found", "status": 404}, 404)

    def do_POST(self):
        if self.path.startswith("/api/v2/auth/login"):
            return self._json(
                {
                    "access_token": "acc",
                    "refresh_token": "ref",
                    "expires_in": 3600,
                    "user": {"id": "1", "username": "mock", "role": "user", "permissions": []},
                }
            )
        if self.path.startswith("/api/v2/auth/refresh"):
            return self._json({"access_token": "acc2", "refresh_token": "ref2", "expires_in": 3600})
        if self.path.startswith("/api/v2/watched/"):
            return self._json({"content_id": "c1", "played": True})
        if self.path.startswith("/api/v2/profiles/") and self.path.endswith("/verify-pin"):
            return self._json({"valid": True, "profile_token": "pvt"})
        return self._json({"type": "about:blank", "title": "not found", "status": 404}, 404)

    def do_DELETE(self):
        return self._json({"content_id": "c1", "played": False})


if __name__ == "__main__":
    ThreadingHTTPServer(("127.0.0.1", PORT), Handler).serve_forever()
