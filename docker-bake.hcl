# Docker Bake definitions for the POCR images.
#
#   docker buildx bake -f docker-bake.hcl              # engine + app + redis
#   docker buildx bake -f docker-bake.hcl engine       # one target
#   docker buildx bake -f docker-bake.hcl --push       # build and push all
#   TAG=v1.2.0 docker buildx bake -f docker-bake.hcl   # override the tag
#   PLATFORMS=linux/amd64,linux/arm64 docker buildx bake -f docker-bake.hcl --push
#
# Always pass `-f docker-bake.hcl`. Without it, bake also auto-loads
# compose.yaml and aborts on its required runtime vars (FHIR_ENV_MAP,
# JWT_SECRET) — which are irrelevant to building the images.
#
# Image names and build targets mirror compose.yaml so bake and
# `compose build` produce identical artifacts.

# Repository prefix shared by every image; the per-target tag is appended.
variable "REGISTRY" {
  default = "andrewdrobinson/ucsf"
}

# Mutable tag applied alongside the fixed pocr-<name> tag. Defaults to latest.
variable "TAG" {
  default = "latest"
}

# Comma-separated build platforms. Single-platform by default so a plain
# `bake` loads into the local docker image store; set multiple for `--push`.
variable "PLATFORMS" {
  default = "linux/amd64"
}

# `bake` with no target builds this group.
group "default" {
  targets = ["engine", "app", "redis"]
}

# Shared settings every target inherits.
target "_common" {
  platforms = split(",", PLATFORMS)
}

target "engine" {
  inherits = ["_common"]
  context  = "./backend"
  target   = "production"
  tags = [
    "${REGISTRY}:pocr-engine",
    "${REGISTRY}:pocr-engine-${TAG}",
  ]
}

target "app" {
  inherits = ["_common"]
  context  = "./frontend"
  tags = [
    "${REGISTRY}:pocr-app",
    "${REGISTRY}:pocr-app-${TAG}",
  ]
}

target "redis" {
  inherits = ["_common"]
  context  = "./redis"
  tags = [
    "${REGISTRY}:pocr-redis",
    "${REGISTRY}:pocr-redis-${TAG}",
  ]
}
