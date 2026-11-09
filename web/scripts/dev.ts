// The dev server: bundles on the fly like `bun ./index.html`, but answers every
// path with index.html, so that a deep link (`/drawings`) opens the app.
import index from "../index.html";

const server = Bun.serve({
  port: process.env.PORT ?? 3000,
  development: true,
  // Development has no service worker; a 404, not the app, answers its registration.
  routes: { "/sw.js": new Response("", { status: 404 }), "/*": index },
});
console.log(`pkm web on ${server.url}`);
