# Deploying the web app

A push to `main` that touches the web app builds `ghcr.io/tim-hilt/pkm`
(`linux/arm64`, tags `latest` and `sha-<commit>`) in
`.github/workflows/web-image.yml`. On the Pi,
[Watchtower](https://watchtower.nickfedor.com) polls ghcr every 5 minutes,
replaces the `pkm` container with the new image, removes the old one and
reports to ntfy.

## The Pi

The services in `~/homelab/compose.yaml`:

```yaml
services:
  pkm:
    image: ghcr.io/tim-hilt/pkm:latest
    container_name: pkm
    restart: unless-stopped
    labels:
      - com.centurylinklabs.watchtower.enable=true

  watchtower:
    image: nickfedor/watchtower:1 # updates itself within v1
    restart: unless-stopped
    volumes:
      - /var/run/docker.sock:/var/run/docker.sock
    environment:
      TZ: Europe/Berlin
      WATCHTOWER_LABEL_ENABLE: "true" # only containers with the label
      WATCHTOWER_POLL_INTERVAL: 300
      WATCHTOWER_CLEANUP: "true"
      WATCHTOWER_NOTIFICATION_URL: ntfy://ntfy.sh/hilt-watchtower-niRwvcE3ofJqiY
    labels:
      - com.centurylinklabs.watchtower.enable=true
```

- **Roll out now**: `docker compose pull pkm && docker compose up -d pkm`.
- **Roll back**: pin `image:` to a `sha-<commit>` tag and run the same
  command; Watchtower leaves it, as that tag never changes.
- **Logs**: `docker compose logs -f watchtower`.
