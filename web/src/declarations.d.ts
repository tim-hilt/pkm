// Assets the bundler turns into URLs.
declare module "*.excalidraw" {
  const url: string;
  export default url;
}
declare module "*.wasm" {
  const url: string;
  export default url;
}
declare module "*.wasm?url" {
  const url: string;
  export default url;
}
declare module "*.excalidraw?url" {
  const url: string;
  export default url;
}
declare module "*.ttf" {
  const url: string;
  export default url;
}
declare module "*.ttf?url" {
  const url: string;
  export default url;
}
