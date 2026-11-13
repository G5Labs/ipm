# Hermetic OAuth/SCM stub for E2E and demo stacks.
# Base: the shared Node tier over the common base
# (software_architecture/integration-stack/docker_images [D18]) rather than
# node:22-alpine, so this sidecar no longer pulls a musl distribution unrelated
# to every other G5 image.
ARG BASE_IMAGE=g5/base-node:26.04
FROM ${BASE_IMAGE} AS final
WORKDIR /app
COPY server.js .
EXPOSE 9876
CMD ["node", "server.js"]
