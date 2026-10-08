#!/usr/bin/env bash
set -euo pipefail
cd "$HOME/Projetos/galaxies"
mkdir -p "$HOME/.cache"
exec 9>"$HOME/.cache/galaxies-deploy.lock"
flock -n 9 || { echo 'Another Galaxies deployment is running.' >&2; exit 1; }
[[ "$(git branch --show-current)" == main ]] || { echo 'Expected main branch.' >&2; exit 1; }
[[ -z "$(git status --porcelain)" ]] || { echo 'Deployment checkout is dirty.' >&2; exit 1; }
git pull --ff-only origin main
echo "Deploying $(git rev-parse HEAD)"
# Build everything before replacing any running containers.
docker compose build
if ! docker compose up -d --wait --wait-timeout 120; then
    docker compose logs --tail=80 >&2
    exit 1
fi
curl --fail --silent --show-error http://127.0.0.1:3020/api/health
curl --fail --silent --show-error --output /dev/null http://127.0.0.1:3020/
docker compose ps
