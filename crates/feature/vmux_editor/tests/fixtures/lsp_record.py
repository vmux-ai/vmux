import json
import sys

while True:
    length = None
    while True:
        line = sys.stdin.buffer.readline()
        if not line:
            sys.exit(0)
        if line in (b"\r\n", b"\n"):
            break
        name, value = line.split(b":", 1)
        if name.lower() == b"content-length":
            length = int(value)
    if length is None:
        sys.exit(1)
    message = json.loads(sys.stdin.buffer.read(length))
    with open("lsp-wire.jsonl", "a", encoding="utf-8") as record:
        record.write(json.dumps(message) + "\n")
    method = message.get("method")
    if method == "exit":
        sys.exit(0)
    if method == "initialize":
        result = {"capabilities": {
            "foldingRangeProvider": True,
            "documentSymbolProvider": True,
            "semanticTokensProvider": {
                "legend": {"tokenTypes": ["type"], "tokenModifiers": []},
                "full": True,
            },
        }}
    elif method == "textDocument/semanticTokens/full":
        result = {"data": []}
    elif method in ("textDocument/foldingRange", "textDocument/documentSymbol"):
        result = []
    elif method == "shutdown":
        result = None
    else:
        continue
    body = json.dumps({"jsonrpc": "2.0", "id": message["id"], "result": result}).encode()
    sys.stdout.buffer.write(f"Content-Length: {len(body)}\r\n\r\n".encode() + body)
    sys.stdout.buffer.flush()
