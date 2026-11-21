#!/bin/sh
# Tests ./fetch against a BusyBox httpd on loopback as the "site" (`make
# web-cgi-test`, CI). Needs BusyBox with httpd, nc and ssl_client: run it in
# the busybox image, not on a Mac.

here=$(cd "$(dirname "$0")" && pwd)
site=$(mktemp -d)
port=18081
base=http://127.0.0.1:$port
failed=0

trap 'kill $server 2>/dev/null; rm -rf "$site"' EXIT

# --- the site: pages, an image, and CGI scripts that redirect or stall.
mkdir "$site/cgi-bin"
echo '<html><title>Hello</title></html>' >"$site/page.html"
printf '\211PNG\r\n\032\n' >"$site/pic.png"
echo 'plain' >"$site/note.txt"
head -c 300000 /dev/zero | tr '\0' x >"$site/big.html"
script() { # name, body
	printf '#!/bin/sh\n%s\n' "$2" >"$site/cgi-bin/$1"
	chmod +x "$site/cgi-bin/$1"
}
script redirect-to-page 'printf "Status: 302\r\nLocation: /page.html\r\n\r\n"'
script redirect-relative 'printf "Status: 301\r\nLocation: ../pic.png\r\n\r\n"'
script redirect-private 'printf "Status: 302\r\nLocation: http://10.0.0.1/secret\r\n\r\n"'
script redirect-loopback "printf 'Status: 302\\r\\nLocation: http://127.0.0.2:$port/page.html\\r\\n\\r\\n'"
script redirect-loop 'printf "Status: 302\r\nLocation: /cgi-bin/redirect-loop\r\n\r\n"'
script redirect-ftp 'printf "Status: 302\r\nLocation: ftp://example.com/x\r\n\r\n"'
# A chunked body: "Hello, " and "chunks" as two chunks.
script chunked 'printf "Content-Type: text/html\r\nTransfer-Encoding: chunked\r\n\r\n7\r\nHello, \r\n6\r\nchunks\r\n0\r\n\r\n"'
script slow 'sleep 10; printf "Content-Type: text/html\r\n\r\nlate"'
busybox httpd -f -p "127.0.0.1:$port" -h "$site" &
server=$!
sleep 1

# --- the harness: runs the script as httpd would.
export PKM_FETCH_ALLOW=127.0.0.1 PKM_FETCH_MAX_BYTES=100000 PKM_FETCH_SECONDS=3
fetch() { # Address
	QUERY_STRING="url=$(printf %s "$1" | sed 's/:/%3A/g; s|/|%2F|g')" sh "$here/fetch" 2>/dev/null | tr -d '\r'
}
check() { # name, Address, expected first line and then expected text anywhere
	out=$(fetch "$2")
	if printf '%s\n' "$out" | head -1 | grep -qx "Status: $3" && printf '%s\n' "$out" | grep -q -- "$4"; then
		echo "ok   $1"
	else
		echo "FAIL $1: wanted $3 / $4, got:"
		printf '%s\n' "$out" | head -8
		failed=1
	fi
}

check "HTML downloads" "$base/page.html" 200 '<title>Hello</title>'
check "HTML keeps its type" "$base/page.html" 200 'Content-Type: text/html'
check "HTML is sandboxed" "$base/page.html" 200 'Content-Security-Policy: sandbox'
check "image downloads" "$base/pic.png" 200 'Content-Type: image/png'
check "chunked body" "$base/cgi-bin/chunked" 200 '^Hello, chunks$'
check "redirect is followed" "$base/cgi-bin/redirect-to-page" 200 '<title>Hello</title>'
check "relative redirect is followed" "$base/cgi-bin/redirect-relative" 200 'Content-Type: image/png'
check "redirect to a private address" "$base/cgi-bin/redirect-private" 403 'private'
check "redirect to loopback" "$base/cgi-bin/redirect-loopback" 403 'private'
check "redirect loop" "$base/cgi-bin/redirect-loop" 502 'too many redirects'
check "redirect to another scheme" "$base/cgi-bin/redirect-ftp" 400 'http and https'
check "other scheme" "ftp://example.com/x" 400 'http and https'
check "file scheme" "file:///etc/passwd" 400 'http and https'
check "user name" "http://user@127.0.0.1/" 400 'user name'
check "no Address" "" 400 'not a valid Address'
check "loopback" "http://127.0.0.2/" 403 'private'
check "private 10/8" "http://10.1.2.3/" 403 'private'
check "private 172.16/12" "http://172.31.0.1/" 403 'private'
check "private 192.168/16" "http://192.168.1.1/" 403 'private'
check "link-local" "http://169.254.169.254/latest" 403 'private'
check "CGNAT" "http://100.64.0.1/" 403 'private'
check "unspecified" "http://0.0.0.0/" 403 'private'
check "IPv6 loopback" "http://[::1]/" 403 'private'
check "IPv6 link-local" "http://[fe80::1]/" 403 'private'
check "IPv6 unique local" "http://[fd00::1]/" 403 'private'
check "IPv4-mapped IPv6" "http://[::ffff:127.0.0.1]/" 403 'private'
check "IPv4 as one number" "http://2130706433/" 400 'not a valid IP'
check "IPv4 with octal octet" "http://010.0.0.1/" 400 'not a valid IP'
check "over the size cap" "$base/big.html" 413 'larger than'
check "not HTML or an image" "$base/note.txt" 415 'only HTML and images'
check "missing page" "$base/nothing.html" 502 'answered 404'
check "over the time cap" "$base/cgi-bin/slow" 504 'longer than'
check "nothing listening" "http://127.0.0.1:1/" 502 'could not download'

# The query as the web app sends it: encodeURIComponent, with other parameters.
out=$(QUERY_STRING="url=http%3A%2F%2F127.0.0.1%3A$port%2Fpage.html%3Fa%3Db&x=1" sh "$here/fetch" | tr -d '\r' | head -1)
[ "$out" = "Status: 200" ] && echo "ok   encoded query" || { echo "FAIL encoded query: $out"; failed=1; }

exit $failed
