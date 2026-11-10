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
- The CPU writes the next frame while the GPU draws the last one: frames no
  longer wait for the GPU as they end, and the first write of the next frame
  waits instead.
- GPUs that share system memory (Apple silicon, Intel integrated graphics)
  get Shared buffers; others keep Managed buffers, now marked modified over
  the range actually written (upstream marked uniforms from offset 0, which
  left later uniforms stale on GPUs with their own memory).
- Integer arguments to Metal calls are passed as 64-bit values (upstream
  passed some as 32-bit, leaving garbage in the upper bits on ARM64).

The macOS window changes: `Platform::apple_low_power_gpu` picks the
integrated GPU on Macs with two (Metal) or allows it (OpenGL), and windows
nobody can see draw at 30 fps instead of 60 with even pacing.

New `RenderingBackend` methods with defaults that keep OpenGL as it was:
`max_render_samples`, `max_anisotropy`, `texture_set_anisotropy`,
`spatial_upscaler_available`, `spatial_upscale`, `prepare_screen_reads` and
`read_screen`.

## luau0-src 0.22.0+luau740

[luau0-src](https://github.com/mlua-rs/luau-src-rs) (MIT, license alongside),
the Luau sources and build logic behind `mlua`, patched so packs written for
the earlier LuaJIT sandbox give exactly the same results. Every change is marked
with a `Tank Yard:` comment; `build.rs` makes Cargo rebuild Luau when these
sources change.

- Parser: Lua 5.3 bitwise operators (`&`, `|`, `~`, `<<`, `>>`, unary `~`)
  with Lua 5.3 precedence, compiled to calls into the sandbox's
  `__tankyard_bitops` (LuaJIT's signed 32-bit results). Explicit type
  instantiation (`f<<T>>`) is not parsed in expressions, so `a << b` stays a
  shift.
- Lexer and parser: hexadecimal fractions and binary exponents (`0x1.8p-3`).
- Numbers print with LuaJIT's `%.14g` instead of Luau's shortest round trip.
- `string.format`: `%a`/`%A`, `%s` through `__tostring`; `%q` escapes control
  characters like LuaJIT. `string.rep` takes a separator; `find`/`match`
  clamp a start past the end; `gsub` treats `%` before a non-digit as that
  character.
- Bounds for C-level work: `string.rep` results to 1 MiB, pattern matching
  to 64 KiB subjects with a quantifier cost limit, `table.sort` to 100,000
  elements, with the script interrupt charged during long sorts.
- `table.sort` is LuaJIT's (Lua 5.1's) quicksort, so equal elements keep the
  same order; `table.insert` below position 1 shifts elements as LuaJIT does.
- `math.max`/`math.min` keep LuaJIT's argument order in the interpreter,
  the fast builtins, constant folding and native code, so NaN and `-0`
  results match.
- `#` ignores `__len` on tables, and `tostring` returns whatever `__tostring`
  returns, as in Lua 5.1 and LuaJIT.
