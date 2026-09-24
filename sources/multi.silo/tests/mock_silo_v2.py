#!/usr/bin/env python3
"""A tiny mock of the Silo v2 API used by the ignored `test_mock_*` tests. It
mirrors silo-server/contracts/api/v2/openapi.json closely enough to catch
contract drift: every authenticated route checks the bearer and profile
headers, and `/catalog` rejects v1 query grammar (`order`, bracketed `groups`)
with a 422.

Usage:
    python3 tests/mock_silo_v2.py [port]
    cargo test -- --ignored test_mock_
"""
import io
import json
import struct
import zlib
from pathlib import Path
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
RAR = (
    Path(__file__).resolve().parents[3]
    / "crates/cbr-native/fixtures/rar40-normal.cbr"
).read_bytes()


def large_plugin_image():
    def chunk(kind, data):
        return struct.pack('>I', len(data)) + kind + data + struct.pack('>I', zlib.crc32(kind + data))
    raw = (b'\x00' + b'\x32\x64\x96' * 1024) * 1024
    return (b'\x89PNG\r\n\x1a\n'
            + chunk(b'IHDR', struct.pack('>IIBBBBB', 1024, 1024, 8, 2, 0, 0, 0))
            + chunk(b'IDAT', zlib.compress(raw, 0)) + chunk(b'IEND', b''))


PLUGIN_IMAGE = large_plugin_image()
PLUGIN_KEY = 'a' * 64
PLUGIN_POLLS = 0

TOKENS = ("Bearer acc", "Bearer acc2", "Bearer mock-api-key")
PROFILES = [
    {"id": "p1", "name": "Main", "has_pin": False, "is_primary": True},
    {"id": "p2", "name": "Locked", "has_pin": True, "is_primary": False},
]
PIN = "1234"
PROFILE_TOKEN = "pvt"
SORT_FIELDS = {"title", "added_at", "release_date", "rating_imdb", "author"}

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

RAR_CHAPTER = {
    "content_id": "cbr1",
    "type": "ebook",
    "title": "RAR Chapter",
    "versions": [
        {
            "file_id": "rar-55",
            "file_name": "cbr1.cbr",
            "container": "cbr",
            "file_size": len(RAR),
            "duration": 3,
        }
    ],
    "series_id": "m1",
    "series_title": "Mock Manga One",
}


def archive_content_type(payload):
    return "application/vnd.comicbook-rar" if payload is RAR else "application/vnd.comicbook+zip"


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
        self.send_header("Content-Type", archive_content_type(payload))
        self.send_header("Content-Length", str(len(payload)))
        self.end_headers()
        self.wfile.write(payload)

    def _problem(self, status, kind):
        self._json({"type": kind, "title": kind, "status": status}, status)

    def _authorized(self, needs_profile=True):
        """Checks the bearer, then the declared profile and its PIN proof."""
        if self.headers.get("Authorization") not in TOKENS:
            self._problem(401, "invalid_token")
            return False
        if not needs_profile:
            return True
        profile = self.headers.get("X-Profile-Id")
        if profile not in ("p1", "p2"):
            self._problem(403, "permission_denied")
            return False
        if profile == "p2" and self.headers.get("X-Profile-Token") != PROFILE_TOKEN:
            self._problem(403, "profile_verification_required")
            return False
        return True

    def _catalog(self, query):
        """Validates the v2 browse grammar and answers from the fixtures."""
        if "order" in query or any(key.startswith("groups[") for key in query):
            return self._problem(422, "validation_failed")
        sort = query.get("sort", [""])[0]
        if sort and sort.lstrip("-") not in SORT_FIELDS:
            return self._problem(422, "invalid_sort_field")
        if query.get("source") == ["section"]:
            if query.get("section_id") != ["recent"] or query.get("library_id") != ["12"]:
                return self._problem(422, "validation_failed")
            return self._json(CATALOG_SECOND)
        if "groups" in query:
            rule = json.loads(query["groups"][0])[0]["rules"][0]
            if (rule["field"], rule["op"]) != ("author", "is"):
                return self._problem(422, "validation_failed")
            items = CATALOG["items"][:1] if rule["value"] == "Mock Author" else []
            return self._json({"page": {"has_more": False}, "items": items, "total": len(items)})
        seek = int(query.get("seek", ["0"])[0])
        response = dict(CATALOG_SECOND if seek > 0 else CATALOG)
        if sort.startswith("-"):
            response["items"] = list(reversed(response["items"]))
        return self._json(response)

    def do_GET(self):
        parsed = urlparse(self.path)
        path = parsed.path
        if path == "/api/v2/system/info":
            return self._json({"server_version": "mock", "api_major": 2})
        if not self._authorized(needs_profile=path != "/api/v2/profiles"):
            return
        if path == "/api/v2/profiles":
            return self._json({"items": PROFILES})
        if path == "/api/v2/settings/plugins":
            return self._json({"items": [{
                "id": "comic-test",
                "plugin_id": "dev.crowquillx.comic-pages",
                "version": "0.2.0",
                "routes": [], "assets": [], "user_config_schema": [],
            }]})
        if path == "/api/v2/user/libraries":
            return self._json(LIBRARIES)
        if path == "/api/v2/catalog":
            return self._catalog(parse_qs(parsed.query))
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
            if content_id == "cbr1":
                return self._json(RAR_CHAPTER)
            return self._json(DETAIL)
        if path.startswith("/api/v2/ebooks/") and path.endswith("/read"):
            payload = RAR if "/cbr1/" in path else CBZ
            full = parse_qs(parsed.query).get("full") == ["1"]
            return self._range_bytes(payload, force_full=full)
        return self._json({"type": "about:blank", "title": "not found", "status": 404}, 404)

    def _range_bytes(self, payload, force_full=False):
        """Serves `payload` honoring a single `bytes=start-end` Range header."""
        header = self.headers.get("Range")
        if force_full or not header or not header.startswith("bytes="):
            return self._bytes(payload)
        start_text, _, end_text = header[len("bytes="):].partition("-")
        total = len(payload)
        start = int(start_text) if start_text else 0
        end = int(end_text) if end_text else total - 1
        start = max(0, min(start, total - 1))
        end = max(start, min(end, total - 1))
        chunk = payload[start:end + 1]
        self.send_response(206)
        self.send_header("Content-Type", archive_content_type(payload))
        self.send_header("Content-Range", f"bytes {start}-{end}/{total}")
        self.send_header("Accept-Ranges", "bytes")
        self.send_header("Content-Length", str(len(chunk)))
        self.end_headers()
        self.wfile.write(chunk)

    def do_POST(self):
        global PLUGIN_POLLS
        path = urlparse(self.path).path
        plugin_base = '/api/v2/plugin-content/plugins/comic-test/v1'
        length = int(self.headers.get('Content-Length', 0))
        if path == "/api/v2/auth/login":
            body = json.loads(self.rfile.read(length))
            if (body.get("username"), body.get("password")) != ("mock", "mock"):
                return self._problem(401, "invalid_credentials")
            return self._json(
                {
                    "access_token": "acc",
                    "refresh_token": "ref",
                    "expires_in": 3600,
                    "user": {"id": "1", "username": "mock", "role": "user", "permissions": []},
                }
            )
        if path == "/api/v2/auth/refresh":
            return self._json({"access_token": "acc2", "refresh_token": "ref2", "expires_in": 3600})
        if path.startswith("/api/v2/profiles/") and path.endswith("/verify-pin"):
            if not self._authorized(needs_profile=False):
                return
            body = json.loads(self.rfile.read(length))
            if body.get("pin") != PIN:
                return self._json({"valid": False, "expires_at": None})
            return self._json({"valid": True, "profile_token": PROFILE_TOKEN,
                               "expires_at": "2099-01-02T15:04:05.000Z"})
        if not self._authorized():
            return
        if path.startswith(plugin_base + '/'):
            body = json.loads(self.rfile.read(length))
            expected = {'token': 'mock-api-key', 'profile_id': 'p1',
                        'content_id': 'cbr1', 'file_id': 'rar-55', 'api_version': 'v2'}
            if any(body.get(k) != v for k, v in expected.items()):
                return self._json({'error': 'incorrect caller credentials'}, 403)
            if path == plugin_base + '/pages':
                PLUGIN_POLLS += 1
                if PLUGIN_POLLS == 1:
                    return self._json({'status': 'preparing'}, 202)
                return self._json({'cache_key': PLUGIN_KEY, 'chunk_bytes': 1048576,
                                   'pages': [{'name': 'page1.png', 'size': len(PLUGIN_IMAGE)}]})
            if path == plugin_base + '/page/0':
                if body.get('cache_key') != PLUGIN_KEY:
                    return self._json({'error': 'reopen chapter'}, 409)
                offset = body.get('offset', 0)
                payload = PLUGIN_IMAGE[offset:offset + 1048576]
                self.send_response(200)
                self.send_header('Content-Type', 'application/octet-stream')
                self.send_header('Content-Length', str(len(payload)))
                self.end_headers()
                self.wfile.write(payload)
                return
            return self._json({'error': 'not found'}, 404)
        if path.startswith("/api/v2/watched/"):
            return self._json({"content_id": "c1", "played": True})
        return self._json({"type": "about:blank", "title": "not found", "status": 404}, 404)

    def do_DELETE(self):
        return self._json({"content_id": "c1", "played": False})


if __name__ == "__main__":
    ThreadingHTTPServer(("127.0.0.1", PORT), Handler).serve_forever()
