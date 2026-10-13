// Remotion's webpack turns an imported font or image into its URL in the bundle.
declare module "*.woff2" {
  const url: string;
  export default url;
}
declare module "*.svg" {
  const url: string;
  export default url;
}
