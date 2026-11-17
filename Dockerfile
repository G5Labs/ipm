ARG BASE=containers.intersystems.com/intersystems/iris-community:2026.1

FROM ${BASE}

# The notary client verifies modules from ORAS repos configured with -trust-server, and the content trust tests sign with it.
# The CA certificate lets it reach the test Notary server over https.
ADD --checksum=sha256:73353b2b4b85604c738a6800465133cb3a828dff0aa26f3c0926dd9a73e19879 --chmod=755 \
    https://github.com/notaryproject/notary/releases/download/v0.6.1/notary-Linux-amd64 /usr/local/bin/notary
COPY --chmod=644 tests/notary/certs/root-ca.crt /usr/local/share/ca-certificates/ipm-test-notary-ca.crt
USER root
RUN update-ca-certificates
USER ${ISC_PACKAGE_MGRUSER}

ARG REGISTRY=https://pm.community.intersystems.com

RUN --mount=type=bind,src=.,dst=/home/irisowner/zpm/ \
    iris start iris && \
    iris session IRIS < /home/irisowner/zpm/iris.script && \
    iris stop iris quietly