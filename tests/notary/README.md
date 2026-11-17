# Notary test trust server configuration

`server-config.json` configures the Docker Content Trust (Notary) server in the `notary` container. `docker-compose.yml` and `.github/workflows/main.yml` both mount it as `/etc/notary/server-config.json`, and `certs/` as `/etc/notary/certs/`.

The server keeps trust data in memory and signs timestamps with keys it generates itself, so it needs no database or signer service. Its trust data is lost when the container restarts. It serves https on port 4443 at `https://notary:4443` on the Docker network.

## Certificates

IPM only accepts an https trust server, as Docker does. `certs/notary-server.crt` and `certs/notary-server.key` are the server's certificate and key for the host name `notary`, issued by `certs/root-ca.crt`. The `iris` image adds `root-ca.crt` to the system's trusted certificates, which the notary client uses. The CA's private key was discarded after issuing the server certificate, so it cannot issue others. The server key exists only for tests and is not secret. The certificates expire in November 2036.

To replace them, issue a new server certificate with `subjectAltName=DNS:notary` from a new CA, replace the three files, and rebuild the `iris` image.

## Verifying modules

An ORAS repo configured with a trust server verifies each module it retrieves with the notary client, which the `iris` image installs as `/usr/local/bin/notary`:

```
repo -o -name signed -url http://oras:5000 -namespace signed -trust-server https://notary:4443
```

The trust data for a module is the collection named after its repository in the registry, `<registry host>/<namespace>/<package>`, as Docker names it. The target name is the registry tag, for example `1.0.0` or `1.0.0_build.1` for version `1.0.0+build.1`. Only targets signed by the collection's `targets/releases` delegation or `targets` role count. A repo with a trust server only offers a package under a name other than its own, such as its external name, when the package's signed module.xml gives that name.

IPM pins the first root it sees for a collection, in `%IPM_Repo_Oras.TrustedCollection`. After recreating a collection on the trust server with new keys, delete its row to accept the new root.

## Signing a published module

IPM does not sign. A promotion process signs a module after `publish`, with the digest and size of the manifest its tag points at. For example, in the `iris` container:

```sh
export NOTARY_ROOT_PASSPHRASE=root-pass NOTARY_TARGETS_PASSPHRASE=targets-pass NOTARY_SNAPSHOT_PASSPHRASE=snapshot-pass
GUN=oras:5000/signed/my-module
TAG=1.0.0
read DIGEST SIZE < <(python3 -c "
import hashlib, urllib.request
request = urllib.request.Request('http://oras:5000/v2/signed/my-module/manifests/$TAG', headers={'Accept': 'application/vnd.oci.image.manifest.v1+json'})
manifest = urllib.request.urlopen(request).read()
print(hashlib.sha256(manifest).hexdigest(), len(manifest))")
notary -s https://notary:4443 -d ~/.notary init $GUN -p   # once per collection
notary -s https://notary:4443 -d ~/.notary addhash $GUN $TAG $SIZE --sha256 $DIGEST -p
```

`Test.PM.Integration.OrasContentTrust` signs this way in each test, in a registry namespace of its own.
