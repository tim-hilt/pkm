# The web app requires WebGPU, with no fallback renderer

The web app renders the Canvas with WebGPU only, as a port of the Metal renderer (ADR-0002, ADR-0008), and shows an unsupported-browser page where WebGPU is missing. A Canvas2D or WebGL fallback was rejected: it would be a second renderer to write, test against reference images and keep in step, for browsers that are fading out; the users' browsers are recent, so the cost is a few older or Linux setups where WebGPU still needs a flag.
