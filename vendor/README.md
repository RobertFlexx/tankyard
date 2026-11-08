# Vendored crates

## miniquad 0.4.11

[miniquad](https://github.com/not-fl3/miniquad) (MIT OR Apache-2.0, licenses
alongside), the windowing and rendering layer under macroquad, with Tank Yard's
Metal improvements for Macs. Every change is marked with a `Tank Yard:`
comment, and the commit after the unmodified import shows them all as a diff.
Other platforms run upstream's OpenGL code unchanged.

The Metal backend changes:

- Pipelines are built for the format and sample count of the pass they draw
  into (upstream always used the window's), so render targets and MSAA work.
- Multisampled render targets and resolve attachments; a multisampled window
  is resolved into the screen (upstream never resolved it).
- Consecutive passes into the same target share one command encoder, since
  macroquad begins and ends a pass for every draw call.
- Viewports, and scissor rectangles flipped with the pass's own height.
- Readback of textures and of the screen, for screenshots.
- Mipmaps for textures made without mip levels, anisotropic filtering, and
  sampler changes that keep the texture's other sampler settings.
- MetalFX spatial upscaling, loaded at run time where macOS and the GPU have it.
- Supported MSAA sample counts are checked, and shader compile errors are
  returned instead of panicking.

New `RenderingBackend` methods with defaults that keep OpenGL as it was:
`max_render_samples`, `max_anisotropy`, `texture_set_anisotropy`,
`spatial_upscaler_available`, `spatial_upscale`, `prepare_screen_reads` and
`read_screen`.
