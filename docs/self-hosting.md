# Self-hosting Cloud Backing with WebDAV

Cloud Backing can keep your Drawings on a WebDAV server you run yourself. The
app creates a folder called `ExcalidrawNative` under the server URL you enter
and keeps every Drawing in it as a plain `.excalidraw` file. Other apps can
share the server: nothing outside `ExcalidrawNative` is touched.

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

The Drawings then live in `/srv/webdav/ExcalidrawNative/`.

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
