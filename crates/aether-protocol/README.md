# aether-protocol

Aether Radio transport header encode / decode / validate.

## API

- `AetherHeader::encode()` → 32-byte LE wire format
- `AetherHeader::decode(&[u8])`
- `AetherHeader::validate()`

## Tests

```bash
cargo test -p aether-protocol
```
