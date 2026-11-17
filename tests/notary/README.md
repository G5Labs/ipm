# Docker Content Trust test server

`server-config.json` configures the `notary` container, a Notary server like the one `docker trust` signs to. `docker-compose.yml` and `.github/workflows/main.yml` both mount it at `/etc/notary/server-config.json`. ORAS repos point at it with `-trust-server http://notary:4443`.

The server keeps trust data in memory and signs timestamps with its own key, so it needs no database or signer service. Restarting it discards all trust data. Since IPM pins the root key of each repository it has verified, delete `ipm/content-trust/` under the IRIS manager directory after a restart.

It runs without TLS and without authentication, like the `oras` test registry. Do not use this configuration outside tests.

## Signing in tests

`Test.PM.Integration.OrasContentTrust` publishes modules with the ordinary `publish` command and then signs them as an external promotion process would, with the notary CLI that the IRIS image installs:

```sh
notary -d <signer keys> -s http://notary:4443 init oras:5000/<namespace>/<module>
notary -d <signer keys> -s http://notary:4443 key rotate oras:5000/<namespace>/<module> snapshot -r
notary -d <signer keys> -s http://notary:4443 addhash oras:5000/<namespace>/<module> <tag> <manifest size> --sha256 <manifest digest> -p
```

The passphrases come from `NOTARY_ROOT_PASSPHRASE`, `NOTARY_TARGETS_PASSPHRASE`, `NOTARY_SNAPSHOT_PASSPHRASE` and `NOTARY_DELEGATION_PASSPHRASE`. The test sets them to fixed values that are not secret.
