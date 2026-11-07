// Assets the bundler turns into URLs.
declare module "*.excalidraw" {
  const url: string;
  export default url;
}
declare module "*.wasm" {
  const url: string;
  export default url;
}
