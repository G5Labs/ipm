ARG BASE=containers.intersystems.com/intersystems/iris-community:2026.1

FROM ${BASE}

ARG REGISTRY=https://pm.community.intersystems.com

ARG TARGETARCH

# The notary CLI verifies packages from ORAS repos configured with -trust-server, and the tests also sign with it.
# v0.6.1 is its last release with prebuilt binaries, and its only Linux build is for amd64.
USER root
RUN if [ "${TARGETARCH:-amd64}" = "amd64" ]; then \
        python3 -c "import hashlib, sys, urllib.request; data = urllib.request.urlopen(sys.argv[1]).read(); assert hashlib.sha256(data).hexdigest() == sys.argv[2], 'notary checksum mismatch'; open('/usr/local/bin/notary', 'wb').write(data)" \
            https://github.com/notaryproject/notary/releases/download/v0.6.1/notary-Linux-amd64 \
            73353b2b4b85604c738a6800465133cb3a828dff0aa26f3c0926dd9a73e19879 && \
        chmod 755 /usr/local/bin/notary; \
    fi
USER ${ISC_PACKAGE_MGRUSER}

RUN --mount=type=bind,src=.,dst=/home/irisowner/zpm/ \
    iris start iris && \
    iris session IRIS < /home/irisowner/zpm/iris.script && \
    iris stop iris quietly