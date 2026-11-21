# The web app fetches pages through a CGI script on its own server

A browser can't fetch another site's page (CORS), so the web app couldn't Save a Bookmark from a pasted Address. The Docker image's BusyBox `httpd` runs a CGI script that downloads a page or image for the web app, one raw request per hop with BusyBox `nc` and `ssl_client`: `wget` can't be stopped from following redirects. Before downloading, it refuses anything but `http(s)`, private and LAN addresses (after every redirect), and anything that isn't HTML or an image. It also caps size and time. Pages Saved from a browser tab come through the bookmarklet instead, as the user sees them.

## Considered Options

- **A Rust binary in the image**: easier address checks, but it needs a TLS crate.
- **Caddy or another proxying server**: about 40 MB more image and less control over what is fetched.
- **Bookmarklet only**: no server code, but no Saving a pasted Address and nothing on phones.

## Consequences

- The web app's server is no longer static files only.
- The image's base is BusyBox (about 0.9 MB instead of a 69 KB bare `httpd`), as the script needs a shell and its applets.
- Downloads are logged out: paywalled or login-only pages give no Article from a pasted Address.
