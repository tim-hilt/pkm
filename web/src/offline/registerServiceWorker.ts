/** Registers the offline worker; the app works without it. Only the production build has one (development serves no `sw.js`). */
export function registerServiceWorker(): void {
  if (process.env.NODE_ENV !== "production" || !("serviceWorker" in navigator)) return;
  navigator.serviceWorker.register("/sw.js").catch(() => {});
}
