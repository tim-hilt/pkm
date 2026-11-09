# Self-hosting Cloud Backing with WebDAV

Cloud Backing can keep your Drawings on a WebDAV server you run yourself. The
app creates a folder called `pkm` under the server URL you enter
and keeps every Drawing in it as a plain `.excalidraw` file. Other apps can
share the server: nothing outside `pkm` is touched.

The app only talks https, so your password never travels in plain text.

## rclone on a Raspberry Pi

1. Install [rclone](https://rclone.org/install/) and make a folder for the
   Drawings: `mkdir -p /srv/webdav`.
2. Serve it on localhost, with a username and a password of your choice:

   ```sh
   rclone serve webdav /srv/webdav --addr 127.0.0.1:8080 --user me --pass 'a long password'
   ```

   Run it as a systemd service to have it start on boot.
3. Open the server to the internet with a [Cloudflare
   Tunnel](https://developers.cloudflare.com/cloudflare-one/connections/connect-networks/):
   install `cloudflared`, create a tunnel and point a hostname of your domain,
   e.g. `dav.example.com`, at `http://127.0.0.1:8080`. Cloudflare provides the
   https certificate, which every Device trusts.
4. In the app, open Settings (⌘, on the Mac, the gear button in the Gallery on
   iPhone and iPad), pick WebDAV, enter `https://dav.example.com`, your
   username and password and tap Connect.

The Drawings then live in `/srv/webdav/pkm/`.

## The web app

The web app talks to the server straight from the browser, so the server has to
allow the web app's origin (CORS). Add `--allow-origin` with the address the web
app is served from, e.g.

```sh
rclone serve webdav /srv/webdav --addr 127.0.0.1:8080 --user me --pass 'a long password' \
  --allow-origin https://draw.example.org
```

Then enter the same `https://dav.example.com`, username and password in the web
app's Settings as in the native apps. Native apps ignore the flag, so one
rclone serves all of them. A server takes one origin: serve the web app from a
single address.

- **Check the flag**: the installed rclone must know it; `rclone serve webdav
  --help | grep allow-origin` prints a line (older releases lack it, update
  rclone).
- **Check the preflight through the tunnel**:

  ```sh
  curl -si -X OPTIONS https://dav.example.com/ \
    -H 'Origin: https://draw.example.org' \
    -H 'Access-Control-Request-Method: PROPFIND' \
    -H 'Access-Control-Request-Headers: authorization,content-type,depth'
  ```

  The answer must be `200` with `Access-Control-Allow-Origin:
  https://draw.example.org`, the methods `PROPFIND`, `MKCOL` and `MOVE`, and
  the headers `Authorization`, `Content-Type`, `Depth`, `Destination` and
  `Overwrite` allowed. Without that the browser refuses the app's requests.
- **Servers that can't be used**: a server that can't allow an origin, such as a
  stock Nextcloud, works in the native apps but not in the web app.
- **Local development**: run a second rclone, on its own folder and tunnel
  hostname (the app only talks https), allowing the dev server's origin:

  ```sh
  rclone serve webdav /tmp/webdav-dev --addr 127.0.0.1:8081 --user me --pass dev \
    --allow-origin http://localhost:3000
  ```

  Point a hostname such as `dav-dev.example.com` at `http://127.0.0.1:8081`,
  and use it in the web app at `http://localhost:3000` (`make web-dev`).

## A certificate of your own

A server with a certificate from your own CA works once the Device trusts that
CA (Keychain Access on the Mac, an installed profile on iPhone and iPad). Apple
platforms refuse certificates valid for more than 825 days, or without the
`serverAuth` extended key usage.

## When Connect fails

- **URL must start with https://**: the app refuses plain http before sending
  anything.
- **Wrong username or password**: the server answered 401.
- **Not a WebDAV server**: something answers at the URL, but not WebDAV.
  Check the path of the URL.
- **Server not reachable, or its certificate is not trusted**: wrong host, no
  network, or a certificate the Device does not trust.
- **Server not reachable, its certificate is not trusted, or the server doesn't
  allow this site (CORS)**: the web app's wording of the previous failure. The
  browser hides which it is, so a server without `--allow-origin` for the web
  app's address looks unreachable; check the preflight as described under
  "The web app".
