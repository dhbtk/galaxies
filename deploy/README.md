# Home production deployment

Public URL: https://galaxies.dhbtk.com

Route 53 (`personal` AWS profile, zone `Z0741380A5QJ4ETJVAJJ`) has a
300-second CNAME to `home-bastion.dhbtk.com`. The bastion's existing Nginx
forwards HTTP and TLS over the VPN to `10.8.0.2`. Home Nginx terminates TLS
and proxies this hostname to `127.0.0.1:3020` in Ubuntu WSL on `desktop.local`.

Compose runs three services: Nginx routes `/api/` and `/images/` to the Rust
backend and other requests to the TanStack Start Node server. Vite builds the
frontend with the Nitro Node adapter; no development server is exposed.
Only the Compose proxy publishes a port, bound to loopback. The tracked
SQLite catalog and images are included in the backend image, so each build
uses the catalog from that Git commit. This is a read-only application dataset,
not a persistent user database.
The catalog is tracked with Git LFS. The home server needs `git-lfs` installed;
deploys run `git lfs pull`, and the image build checks the SQLite file header.

## Deploy

Push to `main`, or manually run **Deploy Galaxies** in GitHub Actions:

```sh
gh workflow run deploy.yml --ref main
```

The production environment contains `PRODUCTION_SSH_PRIVATE_KEY` and
`PRODUCTION_SSH_KNOWN_HOSTS`. Its dedicated SSH key can only execute
`/home/ubuntu/.local/bin/deploy-galaxies-via-desktop` on the bastion. That
script uses a second dedicated key to reach `diana@10.8.0.2:2222`, restricted
to `/home/diana/.local/bin/deploy-galaxies`. No agent forwarding is needed.
Both SSH hops verify host keys.

The deployment script locks the checkout, refuses a dirty checkout or a branch
other than `main`, pulls with `--ff-only`, builds both images before replacing
containers, waits for their health checks, and checks the proxy. Actions then
checks public HTTPS. The deployed commit is printed in the run log. Deploys
use the latest `main` at pull time; brief downtime is possible during replacement.

Checkout: `/home/diana/Projetos/galaxies`. Git uses the `galaxies-github`
SSH alias and a repository-specific read-only GitHub deploy key.

## Operations

Windows SSH is reached with IPv4. Run Linux commands through WSL:

```sh
ssh -4 desktop.local 'wsl -d Ubuntu -- bash -lc "cd ~/Projetos/galaxies && docker compose ps"'
ssh -4 desktop.local 'wsl -d Ubuntu -- bash -lc "cd ~/Projetos/galaxies && docker compose logs --tail=100"'
```

All containers restart unless stopped, and logs rotate at 10 MB with three
files per service. WSL, Docker, the VPN, and host Nginx must be running after
the Windows machine boots, as with the other home deployments.

`host-nginx.conf` is installed at `/etc/nginx/galaxies.conf`, explicitly included
from the host's existing `nginx.conf`. Its certificate is managed by Certbot
using HTTP webroot `/var/www/letsencrypt`; the existing `certbot.timer` renews
it, and a deploy hook validates/reloads Nginx. Certificate keys stay on the host.
Validate with `sudo certbot renew --cert-name galaxies.dhbtk.com --dry-run`.

The files `deploy.sh` and `bastion-deploy.sh` are installed outside the checkout
at the forced-command paths above. Changes to these scripts or host Nginx
configuration require installing those files again; ordinary app updates do not.

For rollback, revert the problematic change on `main` and push; the workflow
rebuilds the reverted version. A failed build leaves running containers alone.
A failed health check is reported, but does not automatically roll back.
