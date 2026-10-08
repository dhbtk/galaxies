#!/usr/bin/env bash
set -euo pipefail
exec ssh -T -p 2222 \
  -o BatchMode=yes -o IdentitiesOnly=yes -o StrictHostKeyChecking=yes \
  -o ServerAliveInterval=15 -o ServerAliveCountMax=4 \
  -i "$HOME/.ssh/galaxies_deploy_to_desktop" diana@10.8.0.2
