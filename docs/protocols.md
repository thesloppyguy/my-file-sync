# Protocol notes

## `/mysync/file/1.0.0`

Request/response (CBOR):

1. Sender → `FileRequest::Offer`
2. Receiver → `FileResponse::Accept` or `Reject`
3. Receiver pulls chunks via `FileRequest::Chunk`
4. Sender replies `FileResponse::Chunk` until `eof`

## `/mysync/sync/1.0.0`

- `SyncRequest::GetManifest { root_id }` → `SyncResponse::Manifest`
- `SyncRequest::PullFile { root_id, relative_path }` → `SyncResponse::FileOffer`

Conflict policy: last-write-wins by `mtime_ms`. Equal mtime + different hash writes ` (conflict remote)` copy.

## Clipboard gossip

Topic: `mysync/clipboard/<userId>`

Payload JSON:

```json
{
  "origin_peer": "...",
  "content_hash": "sha256-hex",
  "text": "...",
  "sent_at_ms": 0
}
```

Receivers ignore messages from their own PeerId and duplicate content hashes.
