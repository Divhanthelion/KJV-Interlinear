"""A stand-in OpenAI-compatible model server for the UI tests (no model, no network).

    python3 tests/ui/mock_llm.py [port]      # serves http://127.0.0.1:8765/v1

GET /v1/models lists one model with a 32k context window. POST /v1/chat/completions
streams reasoning, then a Markdown answer that quotes a reference and reports how
much Scripture was attached, then token usage. A question containing "slow" streams
slowly (for testing Stop); one containing "fail" gets a 500; one containing "long"
streams a long reasoning trace and a long answer (for testing scrolling). With
chat_template_kwargs.enable_thinking false there is no reasoning, as with vLLM.
"""

import json
import re
import sys
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

MODEL = {"id": "mock-model", "object": "model", "max_model_len": 32768}


class Handler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, *args):
        pass

    def send_json(self, status, body):
        data = json.dumps(body).encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def do_GET(self):
        if self.path.rstrip("/") == "/v1/models":
            self.send_json(200, {"object": "list", "data": [MODEL]})
        else:
            self.send_json(404, {"error": {"message": "not found"}})

    def do_POST(self):
        if self.path.rstrip("/") != "/v1/chat/completions":
            return self.send_json(404, {"error": {"message": "not found"}})
        body = json.loads(self.rfile.read(int(self.headers.get("Content-Length", 0))))
        system = body["messages"][0]["content"]
        question = body["messages"][-1]["content"]
        if "fail" in question:
            return self.send_json(500, {"error": {"message": "mock failure"}})
        refs = re.findall(r'<passage ref="([^"]*)"', system)
        scope = "; ".join(refs) if refs else "nothing"
        verses = sum(1 for line in system.splitlines() if line[:1].isdigit() or line.startswith("(title)"))
        answer = (
            f"You attached **{scope}** ({verses} verses).\n\n"
            "- John 11:35 says *Jesus wept.*\n"
            "- Compare Romans 12:15.\n\n"
            f"Your question had {len(question)} characters."
        )
        delay = 0.4 if "slow" in question else 0.01
        thoughts = "Reading the attached text."
        if "long" in question:
            thoughts = " ".join(f"Step {i}: weighing verse {i % 57 + 1} against the question." for i in range(1, 121))
            answer += "".join(f"\n\nParagraph {i}. " + "The text bears this out. " * 12 for i in range(1, 41))
            delay = 0.02
        if body.get("chat_template_kwargs", {}).get("enable_thinking") is False:
            thoughts = ""

        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream")
        self.send_header("Cache-Control", "no-cache")
        self.send_header("Transfer-Encoding", "chunked")
        self.end_headers()

        def event(payload):
            data = f"data: {payload}\n\n".encode()
            self.wfile.write(f"{len(data):x}\r\n".encode() + data + b"\r\n")
            self.wfile.flush()
            time.sleep(delay)

        try:
            for word in thoughts.split(" ") if thoughts else []:
                event(json.dumps({"choices": [{"delta": {"reasoning_content": word + " "}}]}))
            pieces = [answer[i : i + 12] for i in range(0, len(answer), 12)]
            for i, piece in enumerate(pieces):
                last = i == len(pieces) - 1
                event(json.dumps({"choices": [{"delta": {"content": piece}, "finish_reason": "stop" if last else None}]}))
            prompt = len(system) // 4 + len(question) // 4
            event(json.dumps({"choices": [], "usage": {"prompt_tokens": prompt, "completion_tokens": 42}}))
            event("[DONE]")
            self.wfile.write(b"0\r\n\r\n")
        except (BrokenPipeError, ConnectionResetError):
            pass  # the app stopped the answer


if __name__ == "__main__":
    port = int(sys.argv[1]) if len(sys.argv) > 1 else 8765
    ThreadingHTTPServer(("127.0.0.1", port), Handler).serve_forever()
