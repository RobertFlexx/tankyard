#![allow(non_snake_case)]

use crate::native::apple::{
    apple_util::{self, msg_send_},
    frameworks::*,
};

use super::*;

// https://developer.apple.com/metal/Metal-Feature-Set-Tables.pdf
const MAX_UNIFORM_BUFFER_SIZE: u64 = 4 * 1024 * 1024;
const NUM_INFLIGHT_FRAMES: usize = 3;
#[cfg(any(target_os = "macos", all(target_os = "ios", target_arch = "x86_64")))]
const UNIFORM_BUFFER_ALIGN: u64 = 256;
#[cfg(all(target_os = "ios", not(target_arch = "x86_64")))]
const UNIFORM_BUFFER_ALIGN: u64 = 16;

impl From<VertexFormat> for MTLVertexFormat {
    fn from(vf: VertexFormat) -> Self {
        match vf {
            VertexFormat::Float1 => MTLVertexFormat::Float,
            VertexFormat::Float2 => MTLVertexFormat::Float2,
            VertexFormat::Float3 => MTLVertexFormat::Float3,
            VertexFormat::Float4 => MTLVertexFormat::Float4,
            VertexFormat::Byte1 => MTLVertexFormat::UChar,
            VertexFormat::Byte2 => MTLVertexFormat::UChar2,
            VertexFormat::Byte3 => MTLVertexFormat::UChar3,
            VertexFormat::Byte4 => MTLVertexFormat::UChar4,
            VertexFormat::Short1 => MTLVertexFormat::Short,
            VertexFormat::Short2 => MTLVertexFormat::Short2,
            VertexFormat::Short3 => MTLVertexFormat::Short3,
            VertexFormat::Short4 => MTLVertexFormat::Short4,
            VertexFormat::Int1 => MTLVertexFormat::Int,
            VertexFormat::Int2 => MTLVertexFormat::Int2,
            VertexFormat::Int3 => MTLVertexFormat::Int3,
            VertexFormat::Int4 => MTLVertexFormat::Int4,
            VertexFormat::Mat4 => MTLVertexFormat::Float4,
        }
    }
}

impl From<UniformType> for MTLVertexFormat {
    fn from(ut: UniformType) -> Self {
        match ut {
            UniformType::Float1 => MTLVertexFormat::Float,
            UniformType::Float2 => MTLVertexFormat::Float2,
            UniformType::Float3 => MTLVertexFormat::Float3,
            UniformType::Float4 => MTLVertexFormat::Float4,
            UniformType::Int1 => MTLVertexFormat::Int,
            UniformType::Int2 => MTLVertexFormat::Int2,
            UniformType::Int3 => MTLVertexFormat::Int3,
            UniformType::Int4 => MTLVertexFormat::Int4,
            UniformType::Mat4 => MTLVertexFormat::Float4,
        }
    }
}

impl From<Comparison> for MTLCompareFunction {
    fn from(cmp: Comparison) -> Self {
        match cmp {
            Comparison::Never => MTLCompareFunction::Never,
            Comparison::Less => MTLCompareFunction::Less,
            Comparison::LessOrEqual => MTLCompareFunction::LessEqual,
            Comparison::Greater => MTLCompareFunction::Greater,
            Comparison::GreaterOrEqual => MTLCompareFunction::GreaterEqual,
            Comparison::Equal => MTLCompareFunction::Equal,
            Comparison::NotEqual => MTLCompareFunction::NotEqual,
            Comparison::Always => MTLCompareFunction::Always,
        }
    }
}

impl From<BlendFactor> for MTLBlendFactor {
    fn from(factor: BlendFactor) -> Self {
        match factor {
            BlendFactor::Zero => MTLBlendFactor::Zero,
            BlendFactor::One => MTLBlendFactor::One,
            BlendFactor::Value(BlendValue::SourceColor) => MTLBlendFactor::SourceColor,
            BlendFactor::Value(BlendValue::SourceAlpha) => MTLBlendFactor::SourceAlpha,
            BlendFactor::Value(BlendValue::DestinationColor) => MTLBlendFactor::DestinationColor,
            BlendFactor::Value(BlendValue::DestinationAlpha) => MTLBlendFactor::DestinationAlpha,
            BlendFactor::OneMinusValue(BlendValue::SourceColor) => {
                MTLBlendFactor::OneMinusSourceColor
            }
            BlendFactor::OneMinusValue(BlendValue::SourceAlpha) => {
                MTLBlendFactor::OneMinusSourceAlpha
            }
            BlendFactor::OneMinusValue(BlendValue::DestinationColor) => {
                MTLBlendFactor::OneMinusDestinationColor
            }
            BlendFactor::OneMinusValue(BlendValue::DestinationAlpha) => {
                MTLBlendFactor::OneMinusDestinationAlpha
            }
            BlendFactor::SourceAlphaSaturate => MTLBlendFactor::SourceAlphaSaturated,
        }
    }
}

// impl From<StencilOp> for MTLStencilOperation {
//     fn from(op: StencilOp) -> Self {
//         match op {
//             StencilOp::Keep => MTLStencilOperation::Keep,
//             StencilOp::Zero => MTLStencilOperation::Zero,
//             StencilOp::Replace => MTLStencilOperation::Replace,
//             StencilOp::IncrementClamp => MTLStencilOperation::IncrementClamp,
//             StencilOp::DecrementClamp => MTLStencilOperation::DecrementClamp,
//             StencilOp::Invert => MTLStencilOperation::Invert,
//             StencilOp::IncrementWrap => MTLStencilOperation::IncrementWrap,
//             StencilOp::DecrementWrap => MTLStencilOperation::DecrementWrap,
//         }
//     }
// }

impl From<Equation> for MTLBlendOperation {
    fn from(cf: Equation) -> Self {
        match cf {
            Equation::Add => MTLBlendOperation::Add,
            Equation::Subtract => MTLBlendOperation::Subtract,
            Equation::ReverseSubtract => MTLBlendOperation::ReverseSubtract,
        }
    }
}

impl From<CompareFunc> for MTLCompareFunction {
    fn from(cf: CompareFunc) -> Self {
        match cf {
            CompareFunc::Always => MTLCompareFunction::Always,
            CompareFunc::Never => MTLCompareFunction::Never,
            CompareFunc::Less => MTLCompareFunction::Less,
            CompareFunc::Equal => MTLCompareFunction::Equal,
            CompareFunc::LessOrEqual => MTLCompareFunction::LessEqual,
            CompareFunc::Greater => MTLCompareFunction::Greater,
            CompareFunc::NotEqual => MTLCompareFunction::NotEqual,
            CompareFunc::GreaterOrEqual => MTLCompareFunction::GreaterEqual,
        }
    }
}

impl From<VertexStep> for MTLVertexStepFunction {
    fn from(vs: VertexStep) -> Self {
        match vs {
            VertexStep::PerVertex => MTLVertexStepFunction::PerVertex,
            VertexStep::PerInstance => MTLVertexStepFunction::PerInstance,
        }
    }
}

impl From<PrimitiveType> for MTLPrimitiveType {
    fn from(primitive_type: PrimitiveType) -> Self {
        match primitive_type {
            PrimitiveType::Triangles => MTLPrimitiveType::Triangle,
            PrimitiveType::Lines => MTLPrimitiveType::Line,
            PrimitiveType::Points => MTLPrimitiveType::Point,
        }
    }
}

impl From<TextureFormat> for MTLPixelFormat {
    fn from(format: TextureFormat) -> Self {
        match format {
            TextureFormat::RGBA8 => MTLPixelFormat::RGBA8Unorm,
            //TODO: Depth16Unorm ?
            TextureFormat::Depth => MTLPixelFormat::Depth32Float_Stencil8,
            TextureFormat::RGBA16F => MTLPixelFormat::RGBA16Float,
            _ => todo!(),
        }
    }
}

// impl From<CullFace> for MTLCullMode {
//     fn from(cull_face: CullFace) -> Self {
//         match cull_face {
//             CullFace::Back => MTLCullMode::Back,
//             CullFace::Front => MTLCullMode::Front,
//             CullFace::Nothing => MTLCullMode::None,
//         }
//     }
// }

// impl From<FrontFaceOrder> for MTLWinding {
//     fn from(order: FrontFaceOrder) -> Self {
//         match order {
//             FrontFaceOrder::Clockwise => MTLWinding::Clockwise,
//             FrontFaceOrder::CounterClockwise => MTLWinding::CounterClockwise,
//         }
//     }
// }

#[inline]
fn roundup_ub_buffer(current_buffer: u64) -> u64 {
    ((current_buffer) + ((UNIFORM_BUFFER_ALIGN) - 1)) & !((UNIFORM_BUFFER_ALIGN) - 1)
}

// this scenario:
// buffer.update(); draw(buffer); buffer.update(); draw(buffer);
// is very problematic with metal's ownership model.
// buffer.update() half-update buffer - it doesn't really send data to the GPU claiming ownership,
// it postpone reading the CPU memory until drawcall will actually happen.
// There is no way to flush the buffer and makes metal's "update" function to update GPU memory right away.
// Thus miniquad keeps a lot of buffer's copies...
const BUFFERS_IN_ROTATION: usize = 30;

#[derive(Clone, Copy, Debug)]
pub struct Buffer {
    raw: [ObjcId; BUFFERS_IN_ROTATION],
    //buffer_type: BufferType,
    size: usize,
    //index_type: Option<IndexType>,
    value: usize,
    next_value: usize,
}

#[derive(Debug)]
struct ShaderInternal {
    vertex_function: ObjcId,
    fragment_function: ObjcId,
    //uniforms: Vec<ShaderUniform>,
    // the distance, in bytes, between two uniforms in uniforms buffer
    //stride: u64,
}

struct RenderPassInternal {
    render_pass_desc: ObjcId,
    texture: Vec<TextureId>,
    _depth_texture: Option<TextureId>,
    /// Tank Yard: the attachment layout pipelines must match in this pass.
    target: PassTarget,
    /// Tank Yard: resolve multisampled color into the resolve texture.
    resolve: bool,
}

/// Tank Yard: a Metal pipeline state is only valid for one attachment layout
/// (pixel format, sample count, depth). Upstream built every pipeline for the
/// window's layout, which is undefined behaviour in render targets and with
/// multisampling. Pipelines now keep their descriptor and build a state per
/// layout on first use.
#[derive(Clone, Copy, Debug, PartialEq)]
struct PassTarget {
    format: MTLPixelFormat,
    samples: u64,
    depth: bool,
}

/// Tank Yard: which pass the open encoder renders into.
#[derive(Clone, Copy, Debug, PartialEq)]
enum EncoderPass {
    Default,
    Offscreen(RenderPass),
}

#[derive(Clone, Debug)]
struct PipelineInternal {
    /// Tank Yard: retained descriptor, and a state per attachment layout.
    descriptor: ObjcId,
    states: Vec<(PassTarget, ObjcId)>,
    depth_stencil_state: ObjcId,
    //layout: Vec<BufferLayout>,
    //attributes: Vec<VertexAttributeInternal>,
    _shader: ShaderId,
    //params: PipelineParams,
}

#[derive(Clone, Copy)]
struct Texture {
    texture: ObjcId,
    sampler: ObjcId,
    sampler_descriptor: ObjcId,
    params: TextureParams,
}
struct Textures(Vec<Texture>);

impl Textures {
    fn get(&self, texture: TextureId) -> Texture {
        match texture.0 {
            TextureIdInner::Raw(RawId::Metal(_texture)) => unimplemented!(),
            TextureIdInner::Raw(RawId::OpenGl(_)) => panic!("Gl texture in Metal context!"),
            TextureIdInner::Managed(texture) => self.0[texture],
        }
    }

    fn get_mut(&mut self, texture: TextureId) -> &mut Texture {
        match texture.0 {
            TextureIdInner::Raw(RawId::Metal(_texture)) => unimplemented!(),
            TextureIdInner::Raw(RawId::OpenGl(_)) => panic!("Gl texture in Metal context!"),
            TextureIdInner::Managed(texture) => &mut self.0[texture],
        }
    }
}
pub struct MetalContext {
    buffers: Vec<Buffer>,
    shaders: Vec<ShaderInternal>,
    pipelines: Vec<PipelineInternal>,
    textures: Textures,
    passes: Vec<RenderPassInternal>,
    command_queue: ObjcId,
    command_buffer: Option<ObjcId>,
    render_encoder: Option<ObjcId>,
    /// Tank Yard: macroquad begins and ends a pass for every draw call, and a
    /// new encoder stores and reloads every attachment (a multisampled one
    /// included). An ended pass leaves its encoder open; the next pass into
    /// the same target without a clear continues it.
    encoder_pass: Option<EncoderPass>,
    in_pass: bool,
    pass_target: PassTarget,
    /// Tank Yard: the open pass's size in pixels, for viewports and scissors.
    pass_size: (u32, u32),
    view: ObjcId,
    device: ObjcId,
    current_frame_index: usize,
    uniform_buffers: [ObjcId; 3],
    // cached index_buffer from apply_bindings
    index_buffer: Option<ObjcId>,
    // cached pipeline from apply_pipeline
    current_pipeline: Option<Pipeline>,
    current_ub_offset: u64,
    /// Tank Yard: MetalFX spatial upscaling, where the system has it.
    metalfx: Option<&'static objc::runtime::Class>,
    spatial: Option<SpatialScaler>,
}

/// Tank Yard: a MetalFX spatial scaler and the layout it was made for.
struct SpatialScaler {
    scaler: ObjcId,
    key: (u64, u64, u64, u64, MTLPixelFormat, MTLPixelFormat),
}

impl Drop for SpatialScaler {
    fn drop(&mut self) {
        unsafe { msg_send_![self.scaler, release] };
    }
}

/// Tank Yard: load MetalFX (macOS 13 and later) and return its spatial scaler
/// descriptor class if this GPU supports it. Older systems simply lack it.
fn load_metalfx(device: ObjcId) -> Option<&'static objc::runtime::Class> {
    extern "C" {
        fn dlopen(path: *const std::os::raw::c_char, mode: i32) -> *mut std::ffi::c_void;
    }
    const RTLD_LAZY: i32 = 1;
    let path = b"/System/Library/Frameworks/MetalFX.framework/MetalFX\0";
    if unsafe { dlopen(path.as_ptr() as _, RTLD_LAZY) }.is_null() {
        return None;
    }
    let class = objc::runtime::Class::get("MTLFXSpatialScalerDescriptor")?;
    let supported: BOOL = unsafe { msg_send![class, supportsDevice: device] };
    (supported == YES).then_some(class)
}

impl Default for MetalContext {
    fn default() -> Self {
        Self::new()
    }
}

impl MetalContext {
    pub fn new() -> MetalContext {
        unsafe {
            let view = crate::window::apple_view();
            assert!(!view.is_null());
            let device: ObjcId = msg_send![view, device];
            assert!(!device.is_null());
            let command_queue: ObjcId = msg_send![device, newCommandQueue];

            if false {
                let capture_manager = msg_send_![class![MTLCaptureManager], sharedCaptureManager];
                assert!(!capture_manager.is_null());

                let MTLCaptureDestinationGPUTraceDocument = 2u64;
                if !msg_send![
                    capture_manager,
                    supportsDestination: MTLCaptureDestinationGPUTraceDocument
                ] {
                    // put this into Info.plist near the binary (in target/debug/whatever/whatever, not in the working directory)
                    /*
                    <?xml version="1.0" encoding="UTF-8"?>
                    <!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
                    <plist version="1.0">
                    <dict>
                        <key>MetalCaptureEnabled</key>
                        <true/>
                    </dict>
                    </plist>
                    */
                    panic!("capture failed (probably missing Info.plist, read the comment near this panic)");
                }

                let capture_descriptor =
                    msg_send_![msg_send_![class![MTLCaptureDescriptor], alloc], init];
                msg_send_![capture_descriptor, setCaptureObject: device];
                msg_send_![
                    capture_descriptor,
                    setDestination: MTLCaptureDestinationGPUTraceDocument
                ];
                let path = apple_util::str_to_nsstring("/Users/fedor/wtf1.gputrace");
                let url = msg_send_![class!(NSURL), fileURLWithPath: path];
                msg_send_![capture_descriptor, setOutputURL: url];

                let mut error: ObjcId = nil;
                if !msg_send![capture_manager, startCaptureWithDescriptor:capture_descriptor
                              error:&mut error]
                {
                    let description: ObjcId = msg_send![error, localizedDescription];
                    let string = apple_util::nsstring_to_string(description);
                    panic!("Capture error: {}", string);
                }
            }

            #[cfg(target_os = "macos")]
            let options = {
                MTLResourceOptions::CPUCacheModeWriteCombined
                    | MTLResourceOptions::StorageModeManaged
            };
            #[cfg(target_os = "ios")]
            let options = { MTLResourceOptions::CPUCacheModeWriteCombined };

            let uniform_buffers = [
                msg_send![device, newBufferWithLength:MAX_UNIFORM_BUFFER_SIZE
                          options:options],
                msg_send![device, newBufferWithLength:MAX_UNIFORM_BUFFER_SIZE
                          options:options],
                msg_send![device, newBufferWithLength:MAX_UNIFORM_BUFFER_SIZE
                          options:options],
            ];

            MetalContext {
                command_queue,
                command_buffer: None,
                render_encoder: None,
                encoder_pass: None,
                in_pass: false,
                pass_target: PassTarget {
                    format: MTLPixelFormat::BGRA8Unorm,
                    samples: 1,
                    depth: true,
                },
                pass_size: (1, 1),
                view,
                device,
                buffers: vec![],
                shaders: vec![],
                pipelines: vec![],
                textures: Textures(Vec::new()),
                passes: vec![],
                index_buffer: None,
                current_pipeline: None,
                uniform_buffers,
                current_frame_index: 1,
                current_ub_offset: 0,
                metalfx: load_metalfx(device),
                spatial: None,
            }
        }
    }
}

impl MetalContext {
    /// Tank Yard: the pipeline's state for the current pass layout.
    fn pipeline_state(&mut self, pipeline: usize) -> ObjcId {
        let target = self.pass_target;
        let pipeline = &mut self.pipelines[pipeline];
        if let Some((_, state)) = pipeline.states.iter().find(|(t, _)| *t == target) {
            return *state;
        }
        unsafe {
            let descriptor = pipeline.descriptor;
            let attachments = msg_send_![descriptor, colorAttachments];
            let color = msg_send_![attachments, objectAtIndexedSubscript: 0u64];
            msg_send_![color, setPixelFormat: target.format];
            let unused = msg_send_![attachments, objectAtIndexedSubscript: 1u64];
            msg_send_![unused, setPixelFormat: MTLPixelFormat::Invalid];
            msg_send_![descriptor, setRasterSampleCount: target.samples];
            let depth = if target.depth {
                MTLPixelFormat::Depth32Float_Stencil8
            } else {
                MTLPixelFormat::Invalid
            };
            msg_send_![descriptor, setDepthAttachmentPixelFormat: depth];
            msg_send_![descriptor, setStencilAttachmentPixelFormat: depth];
            let mut error: ObjcId = nil;
            let state: ObjcId = msg_send![
                self.device,
                newRenderPipelineStateWithDescriptor: descriptor
                error: &mut error
            ];
            if state.is_null() {
                let description: ObjcId = msg_send![error, localizedDescription];
                let string = apple_util::nsstring_to_string(description);
                panic!("newRenderPipelineStateWithDescriptor error: {}", string);
            }
            pipeline.states.push((target, state));
            state
        }
    }

    /// Tank Yard: finish the open encoder, if any.
    fn close_encoder(&mut self) {
        if let Some(encoder) = self.render_encoder.take() {
            unsafe { msg_send_!(encoder, endEncoding) };
        }
        self.encoder_pass = None;
        self.index_buffer = None;
    }

    /// Tank Yard: replace a texture's sampler after its descriptor changed.
    fn rebuild_sampler(device: ObjcId, texture: &mut Texture) {
        unsafe {
            let previous = texture.sampler;
            texture.sampler = msg_send_![
                device,
                newSamplerStateWithDescriptor: texture.sampler_descriptor
            ];
            if !previous.is_null() {
                msg_send_![previous, release];
            }
        }
    }

    /// Tank Yard: copy a texture's pixels (4 bytes each, top row first).
    fn read_texture(&mut self, texture: ObjcId, width: u32, height: u32) -> Vec<u8> {
        self.close_encoder();
        let row = u64::from(width) * 4;
        let length = row * u64::from(height);
        let mut pixels = vec![0u8; length as usize];
        unsafe {
            let buffer: ObjcId = msg_send![self.device,
                newBufferWithLength: length
                options: MTLResourceOptions::StorageModeShared];
            let command_buffer = self.command_buffer();
            let blit = msg_send_![command_buffer, blitCommandEncoder];
            msg_send_![blit, copyFromTexture: texture
                sourceSlice: 0u64
                sourceLevel: 0u64
                sourceOrigin: MTLOrigin { x: 0, y: 0, z: 0 }
                sourceSize: MTLSize { width: u64::from(width), height: u64::from(height), depth: 1 }
                toBuffer: buffer
                destinationOffset: 0u64
                destinationBytesPerRow: row
                destinationBytesPerImage: length];
            msg_send_![blit, endEncoding];
            msg_send_![command_buffer, commit];
            msg_send_![command_buffer, waitUntilCompleted];
            self.command_buffer = None;
            let contents: *const u8 = msg_send![buffer, contents];
            std::ptr::copy_nonoverlapping(contents, pixels.as_mut_ptr(), pixels.len());
            msg_send_![buffer, release];
        }
        pixels
    }

    fn command_buffer(&mut self) -> ObjcId {
        if self.command_buffer.is_none() {
            self.command_buffer = Some(unsafe { msg_send![self.command_queue, commandBuffer] });
        }
        self.command_buffer.unwrap()
    }
}

impl RenderingBackend for MetalContext {
    fn info(&self) -> ContextInfo {
        ContextInfo {
            backend: Backend::Metal,
            gl_version_string: Default::default(),
            glsl_support: Default::default(),
            features: Features {
                instancing: true,
                // Tank Yard: resolve attachments are implemented.
                resolve_attachments: true,
            },
        }
    }
    fn buffer_size(&mut self, buffer: BufferId) -> usize {
        let buffer = &self.buffers[buffer.0];
        buffer.size
    }
    fn delete_buffer(&mut self, buffer: BufferId) {
        let buffer = &self.buffers[buffer.0];
        unsafe {
            for buffer in &buffer.raw {
                msg_send_![*buffer, release];
            }
        }
    }
    fn delete_texture(&mut self, texture: TextureId) {
        let texture = self.textures.get(texture);
        unsafe {
            msg_send_![texture.texture, release];
        }
    }
    // Tank Yard: both take OpenGL's bottom-up coordinates and flip them with
    // the open pass's height; upstream used the window's, which put scissors
    // in the wrong place in offscreen targets. Encoders now outlive a single
    // draw, so every draw sets both.
    fn apply_viewport(&mut self, x: i32, y: i32, w: i32, h: i32) {
        let encoder = self.render_encoder.expect("viewport outside a pass");
        let height = self.pass_size.1 as f64;
        let viewport = MTLViewport {
            originX: x as f64,
            originY: height - (y + h) as f64,
            width: w.max(0) as f64,
            height: h.max(0) as f64,
            znear: 0.,
            zfar: 1.,
        };
        unsafe { msg_send_![encoder, setViewport: viewport] };
    }
    fn apply_scissor_rect(&mut self, x: i32, y: i32, w: i32, h: i32) {
        let encoder = self.render_encoder.expect("scissor outside a pass");
        // Metal requires the rectangle to lie inside the target.
        let (width, height) = (self.pass_size.0 as i64, self.pass_size.1 as i64);
        let top = height - (y as i64 + h as i64);
        let left = (x as i64).clamp(0, width);
        let top_clamped = top.clamp(0, height);
        let right = (x as i64 + w.max(0) as i64).clamp(left, width);
        let bottom = (top + h.max(0) as i64).clamp(top_clamped, height);
        let r = MTLScissorRect {
            x: left as u64,
            y: top_clamped as u64,
            width: (right - left) as u64,
            height: (bottom - top_clamped) as u64,
        };
        unsafe { msg_send_![encoder, setScissorRect: r] };
    }
    fn texture_set_min_filter(
        &mut self,
        texture: TextureId,
        filter: FilterMode,
        mipmap_filter: MipmapFilterMode,
    ) {
        let texture = self.textures.get_mut(texture);

        let filter = match filter {
            FilterMode::Nearest => MTLSamplerMinMagFilter::Nearest,
            FilterMode::Linear => MTLSamplerMinMagFilter::Linear,
        };

        let mipmap_filter = match mipmap_filter {
            MipmapFilterMode::None => MTLSamplerMipFilter::NotMipmapped,
            MipmapFilterMode::Nearest => MTLSamplerMipFilter::Nearest,
            MipmapFilterMode::Linear => MTLSamplerMipFilter::Linear,
        };

        // Tank Yard: keep the texture's other sampler settings (upstream
        // started from a fresh descriptor, resetting magnification and wrap).
        unsafe {
            msg_send_![texture.sampler_descriptor, setMinFilter: filter];
            msg_send_![texture.sampler_descriptor, setMipFilter: mipmap_filter];
        }
        Self::rebuild_sampler(self.device, texture);
    }
    fn texture_set_mag_filter(&mut self, texture: TextureId, filter: FilterMode) {
        let texture = self.textures.get_mut(texture);

        let filter = match filter {
            FilterMode::Nearest => MTLSamplerMinMagFilter::Nearest,
            FilterMode::Linear => MTLSamplerMinMagFilter::Linear,
        };

        unsafe { msg_send_![texture.sampler_descriptor, setMagFilter: filter] };
        Self::rebuild_sampler(self.device, texture);
    }
    fn texture_set_wrap(&mut self, texture: TextureId, wrap_x: TextureWrap, wrap_y: TextureWrap) {
        let texture = self.textures.get_mut(texture);

        let wrap_s = match wrap_x {
            TextureWrap::Repeat => MTLSamplerAddressMode::Repeat,
            TextureWrap::Mirror => MTLSamplerAddressMode::MirrorRepeat,
            TextureWrap::Clamp => MTLSamplerAddressMode::ClampToEdge,
        };

        let wrap_t = match wrap_y {
            TextureWrap::Repeat => MTLSamplerAddressMode::Repeat,
            TextureWrap::Mirror => MTLSamplerAddressMode::MirrorRepeat,
            TextureWrap::Clamp => MTLSamplerAddressMode::ClampToEdge,
        };

        unsafe {
            msg_send_![texture.sampler_descriptor, setSAddressMode: wrap_s];
            msg_send_![texture.sampler_descriptor, setTAddressMode: wrap_t];
        }
        Self::rebuild_sampler(self.device, texture);
    }
    fn max_anisotropy(&self) -> Option<u8> {
        // Tank Yard: every Metal GPU filters up to 16x.
        Some(16)
    }
    fn texture_set_anisotropy(&mut self, texture: TextureId, level: u8) -> bool {
        let texture = self.textures.get_mut(texture);
        let level = u64::from(level.clamp(1, 16));
        unsafe { msg_send_![texture.sampler_descriptor, setMaxAnisotropy: level] };
        Self::rebuild_sampler(self.device, texture);
        true
    }
    fn texture_resize(
        &mut self,
        _texture: TextureId,
        _width: u32,
        _height: u32,
        _bytes: Option<&[u8]>,
    ) {
        unimplemented!()
    }
    fn texture_read_pixels(&mut self, texture: TextureId, bytes: &mut [u8]) {
        // Tank Yard: implemented with a blit into shared memory.
        let texture = self.textures.get(texture);
        let (width, height) = (texture.params.width, texture.params.height);
        assert!(bytes.len() >= (width * height * 4) as usize);
        let pixels = self.read_texture(texture.texture, width, height);
        bytes[..pixels.len()].copy_from_slice(&pixels);
    }
    fn texture_generate_mipmaps(&mut self, texture: TextureId) {
        // Tank Yard: a blit encoder needs the render encoder closed.
        self.close_encoder();
        unsafe {
            let command_buffer = self.command_buffer();
            let encoder = msg_send_![command_buffer, blitCommandEncoder];
            // Tank Yard: OpenGL can add mip levels to any texture; Metal
            // allocates them up front. Move a texture made without them into
            // one with them, under the same id.
            let internal = self.textures.get(texture);
            let levels: u64 = msg_send![internal.texture, mipmapLevelCount];
            let (width, height) = (
                u64::from(internal.params.width),
                u64::from(internal.params.height),
            );
            if levels <= 1 && internal.params.sample_count <= 1 && width.max(height) > 1 {
                let format: MTLPixelFormat = msg_send![internal.texture, pixelFormat];
                let descriptor = msg_send_![class!(MTLTextureDescriptor),
                    texture2DDescriptorWithPixelFormat: format
                    width: width
                    height: height
                    mipmapped: YES];
                let storage: u64 = msg_send![internal.texture, storageMode];
                let usage: u64 = msg_send![internal.texture, usage];
                msg_send_![descriptor, setStorageMode: storage];
                msg_send_![descriptor, setUsage: usage];
                let mipmapped: ObjcId = msg_send![self.device, newTextureWithDescriptor: descriptor];
                if !mipmapped.is_null() {
                    let origin = MTLOrigin { x: 0, y: 0, z: 0 };
                    msg_send_![encoder, copyFromTexture: internal.texture
                        sourceSlice: 0u64
                        sourceLevel: 0u64
                        sourceOrigin: origin
                        sourceSize: MTLSize { width, height, depth: 1 }
                        toTexture: mipmapped
                        destinationSlice: 0u64
                        destinationLevel: 0u64
                        destinationOrigin: origin];
                    // The command buffer keeps the old texture alive until
                    // the copy runs.
                    msg_send_![internal.texture, release];
                    let slot = self.textures.get_mut(texture);
                    slot.texture = mipmapped;
                    slot.params.allocate_mipmaps = true;
                }
            }
            let texture = self.textures.get(texture);
            msg_send_![encoder, generateMipmapsForTexture: texture.texture];
            msg_send_![encoder, endEncoding];
        }
    }
    fn texture_params(&self, texture: TextureId) -> TextureParams {
        let texture = self.textures.get(texture);
        texture.params
    }
    unsafe fn texture_raw_id(&self, texture: TextureId) -> RawId {
        let texture = self.textures.get(texture);
        RawId::Metal(texture.texture)
    }

    fn clear(
        &mut self,
        color: Option<(f32, f32, f32, f32)>,
        depth: Option<f32>,
        stencil: Option<i32>,
    ) {
        // TODO: begin_pass/end_pass works, but is far from optimal
        self.begin_default_pass(PassAction::Clear {
            color,
            depth,
            stencil,
        });
        self.end_render_pass();
    }

    fn new_render_pass_mrt(
        &mut self,
        color_img: &[TextureId],
        resolve_img: Option<&[TextureId]>,
        depth_img: Option<TextureId>,
    ) -> RenderPass {
        unsafe {
            let render_pass_desc =
                msg_send_![class!(MTLRenderPassDescriptor), renderPassDescriptor];
            msg_send_![render_pass_desc, retain];
            assert!(!render_pass_desc.is_null());
            for (i, color_img) in color_img.iter().enumerate() {
                let color_texture = self.textures.get(*color_img).texture;
                let color_attachment = msg_send_![msg_send_![render_pass_desc, colorAttachments], objectAtIndexedSubscript:i];
                msg_send_![color_attachment, setTexture: color_texture];
                msg_send_![color_attachment, setLoadAction: MTLLoadAction::Clear];
                msg_send_![color_attachment, setStoreAction: MTLStoreAction::Store];
                // Tank Yard: resolve attachments.
                if let Some(resolve) = resolve_img.and_then(|resolve| resolve.get(i)) {
                    let resolve_texture = self.textures.get(*resolve).texture;
                    msg_send_![color_attachment, setResolveTexture: resolve_texture];
                }
            }
            if let Some(depth_img) = depth_img {
                let depth_texture = self.textures.get(depth_img).texture;

                let depth_attachment = msg_send_![render_pass_desc, depthAttachment];
                msg_send_![depth_attachment, setTexture: depth_texture];
                msg_send_![depth_attachment, setLoadAction: MTLLoadAction::Clear];
                msg_send_![depth_attachment, setStoreAction: MTLStoreAction::Store];
                msg_send_![depth_attachment, setClearDepth:1.];

                let stencil_attachment = msg_send_![render_pass_desc, stencilAttachment];
                msg_send_![stencil_attachment, setTexture: depth_texture];
            }
            let params = color_img
                .first()
                .map(|color| self.textures.get(*color).params)
                .unwrap_or_default();
            let pass = RenderPassInternal {
                render_pass_desc,
                texture: color_img.to_vec(),
                _depth_texture: depth_img,
                target: PassTarget {
                    format: params.format.into(),
                    samples: params.sample_count.max(1) as u64,
                    depth: depth_img.is_some(),
                },
                resolve: resolve_img.is_some(),
            };

            self.passes.push(pass);

            RenderPass(self.passes.len() - 1)
        }
    }

    fn delete_render_pass(&mut self, render_pass: RenderPass) {
        // Tank Yard: never release a pass an open encoder still renders into.
        if self.encoder_pass == Some(EncoderPass::Offscreen(render_pass)) {
            self.close_encoder();
        }
        let render_pass = &self.passes[render_pass.0];
        unsafe {
            msg_send_![render_pass.render_pass_desc, release];
        }
    }

    fn render_pass_color_attachments(&self, render_pass: RenderPass) -> &[TextureId] {
        &self.passes[render_pass.0].texture
    }

    fn new_buffer(&mut self, _: BufferType, _usage: BufferUsage, data: BufferSource) -> BufferId {
        let mut raw = [nil; BUFFERS_IN_ROTATION];
        let size = match &data {
            BufferSource::Slice(data) => data.size,
            BufferSource::Empty { size, .. } => *size,
        };
        #[allow(clippy::needless_range_loop)]
        for i in 0..BUFFERS_IN_ROTATION {
            let buffer: ObjcId = if let BufferSource::Slice(data) = &data {
                debug_assert!(data.is_slice);
                let size = data.size as u64;
                unsafe {
                    msg_send![self.device,
                              newBufferWithBytes:data.ptr
                              length:size
                              options:MTLResourceOptions::StorageModeShared]
                }
            } else {
                #[cfg(target_os = "macos")]
                let options = MTLResourceOptions::CPUCacheModeWriteCombined
                    | MTLResourceOptions::StorageModeManaged;
                #[cfg(target_os = "ios")]
                let options = MTLResourceOptions::CPUCacheModeWriteCombined;
                unsafe {
                    msg_send![self.device,
                              newBufferWithLength:size
                              options:options]
                }
            };

            unsafe {
                msg_send_![buffer, retain];
            }
            raw[i] = buffer;
        }
        let buffer = Buffer {
            raw,
            size,
            value: 0,
            next_value: 0,
        };
        self.buffers.push(buffer);
        BufferId(self.buffers.len() - 1)
    }

    fn buffer_update(&mut self, buffer: BufferId, data: BufferSource) {
        let data = match data {
            BufferSource::Slice(data) => data,
            _ => panic!("buffer_update expects BufferSource::slice"),
        };
        let buffer = &mut self.buffers[buffer.0];
        assert!(data.size <= buffer.size);

        unsafe {
            let dest: *mut std::ffi::c_void = msg_send![buffer.raw[buffer.next_value], contents];
            std::ptr::copy(data.ptr, dest, data.size);

            #[cfg(target_os = "macos")]
            msg_send_![buffer.raw[buffer.next_value], didModifyRange:NSRange::new(0, data.size as u64)];
        }
        buffer.value = buffer.next_value;
    }

    fn new_shader(
        &mut self,
        shader: ShaderSource,
        _meta: ShaderMeta,
    ) -> Result<ShaderId, ShaderError> {
        unsafe {
            let program = match shader {
                ShaderSource::Msl { program } => program,
                _ => panic!("OpenGl source on Metal context"),
            };
            let shader = apple_util::str_to_nsstring(program);
            let mut error: ObjcId = nil;
            let library: ObjcId = msg_send![
                self.device,
                newLibraryWithSource: shader
                options:nil
                error: &mut error
            ];
            // Tank Yard: report compile errors instead of panicking, so callers
            // can fall back as they do on OpenGL.
            if library.is_null() {
                let description: ObjcId = msg_send![error, localizedDescription];
                let string = apple_util::nsstring_to_string(description);
                return Err(ShaderError::CompilationError {
                    shader_type: ShaderType::Fragment,
                    error_message: string,
                });
            }

            let vertex_function: ObjcId = msg_send![library, newFunctionWithName: apple_util::str_to_nsstring("vertexShader")];
            let fragment_function: ObjcId = msg_send![library, newFunctionWithName: apple_util::str_to_nsstring("fragmentShader")];
            if vertex_function.is_null() || fragment_function.is_null() {
                return Err(ShaderError::LinkError(
                    "vertexShader and fragmentShader are required".into(),
                ));
            }
            let shader = ShaderInternal {
                vertex_function,
                fragment_function,
            };
            self.shaders.push(shader);
            Ok(ShaderId(self.shaders.len() - 1))
        }
    }

    fn new_texture(
        &mut self,
        access: TextureAccess,
        bytes: TextureSource,
        params: TextureParams,
    ) -> TextureId {
        let descriptor = unsafe {
            msg_send_![class!(MTLTextureDescriptor),
                       texture2DDescriptorWithPixelFormat:MTLPixelFormat::from(params.format)
                       width: params.width as u64
                       height: params.height as u64
                       mipmapped: params.allocate_mipmaps as BOOL]
        };

        // unsafe {
        //     msg_send_![descriptor, retain];
        // }
        unsafe {
            msg_send_![descriptor, setCpuCacheMode: MTLCPUCacheMode::DefaultCache];

            if access == TextureAccess::RenderTarget {
                if params.format != TextureFormat::Depth {
                    let pixel_format: MTLPixelFormat = params.format.into();
                    msg_send_![descriptor, setPixelFormat: pixel_format];
                }
                msg_send_![descriptor, setStorageMode: MTLStorageMode::Private];
                msg_send_![
                    descriptor,
                    setUsage: MTLTextureUsage::RenderTarget as u64
                        | MTLTextureUsage::ShaderRead as u64
                        | MTLTextureUsage::ShaderWrite as u64
                ];
                // Tank Yard: multisampled render targets.
                if params.sample_count > 1 {
                    msg_send_![descriptor, setTextureType: MTLTextureType::D2Multisample];
                    msg_send_![descriptor, setSampleCount: params.sample_count as u64];
                    msg_send_![
                        descriptor,
                        setUsage: MTLTextureUsage::RenderTarget as u64
                    ];
                }
            } else {
                #[cfg(target_os = "macos")]
                {
                    msg_send_![descriptor, setUsage: MTLTextureUsage::ShaderRead];
                    msg_send_![descriptor, setStorageMode: MTLStorageMode::Managed];
                    msg_send_![
                        descriptor,
                        setResourceOptions: MTLResourceOptions::StorageModeManaged
                    ];
                }
                #[cfg(target_os = "ios")]
                {
                    msg_send_![descriptor, setStorageMode: MTLStorageMode::Shared];
                    msg_send_![
                        descriptor,
                        setResourceOptions: MTLResourceOptions::StorageModeShared
                    ];
                }
            }
        };

        match params.kind {
            TextureKind::Texture2D => {
                // on metal textureType2D is the default, nothing to do here
            }
            TextureKind::CubeMap => unsafe {
                msg_send_![descriptor, setTextureType: MTLTextureType::CubeArray];
            },
        }

        let texture = unsafe {
            let sampler_descriptor = msg_send_![class!(MTLSamplerDescriptor), new];
            msg_send_![sampler_descriptor, retain];
            let min_filter = match params.min_filter {
                FilterMode::Nearest => MTLSamplerMinMagFilter::Nearest,
                FilterMode::Linear => MTLSamplerMinMagFilter::Linear,
            };

            let mag_filter = match params.mag_filter {
                FilterMode::Nearest => MTLSamplerMinMagFilter::Nearest,
                FilterMode::Linear => MTLSamplerMinMagFilter::Linear,
            };

            let mipmap_filter = match params.mipmap_filter {
                MipmapFilterMode::None => MTLSamplerMipFilter::NotMipmapped,
                MipmapFilterMode::Nearest => MTLSamplerMipFilter::Nearest,
                MipmapFilterMode::Linear => MTLSamplerMipFilter::Linear,
            };
            msg_send_![sampler_descriptor, setMinFilter: min_filter];
            msg_send_![sampler_descriptor, setMagFilter: mag_filter];
            msg_send_![sampler_descriptor, setMipFilter: mipmap_filter];

            let sampler_state = msg_send_![
                self.device,
                newSamplerStateWithDescriptor: sampler_descriptor
            ];
            let raw_texture = msg_send_![self.device, newTextureWithDescriptor: descriptor];
            msg_send_![raw_texture, retain];
            self.textures.0.push(Texture {
                sampler: sampler_state,
                texture: raw_texture,
                sampler_descriptor,
                params,
            });
            TextureId(TextureIdInner::Managed(self.textures.0.len() - 1))
        };

        match bytes {
            TextureSource::Empty => {}
            TextureSource::Bytes(bytes) => {
                assert_eq!(
                    params.format.size(params.width, params.height) as usize,
                    bytes.len()
                );

                self.texture_update_part(
                    texture,
                    0,
                    0,
                    params.width as _,
                    params.height as _,
                    bytes,
                );
            }
            TextureSource::Array(array) => {
                for (n, face) in array.iter().enumerate() {
                    for (mipmap_level, bytes) in face.iter().enumerate() {
                        let raw_texture = self.textures.get(texture).texture;
                        let region = MTLRegion {
                            origin: MTLOrigin {
                                x: 0_u64,
                                y: 0_u64,
                                z: 0,
                            },
                            size: MTLSize {
                                width: params.width as u64,
                                height: params.height as u64,
                                depth: 1,
                            },
                        };
                        assert!(bytes.len() as u32 == params.width * params.height * 4);
                        unsafe {
                            msg_send_![raw_texture, replaceRegion:region
                                  mipmapLevel:mipmap_level
                                  slice: n
                                  withBytes:bytes.as_ptr()
                                  bytesPerRow:(params.width * 4) as u64
                                  bytesPerImage:0
                            ];
                        }
                    }
                }
            }
        }
        texture
    }

    fn texture_update_part(
        &mut self,
        texture: TextureId,
        x_offset: i32,
        y_offset: i32,
        width: i32,
        height: i32,
        bytes: &[u8],
    ) {
        let raw_texture = self.textures.get(texture).texture;
        let region = MTLRegion {
            origin: MTLOrigin {
                x: x_offset as u64,
                y: y_offset as u64,
                z: 0,
            },
            size: MTLSize {
                width: width as u64,
                height: height as u64,
                depth: 1,
            },
        };
        unsafe {
            msg_send_![raw_texture, replaceRegion:region
                       mipmapLevel:0
                       withBytes:bytes.as_ptr()
                       bytesPerRow:(width * 4) as u64];
        }
    }

    fn new_pipeline(
        &mut self,
        buffer_layout: &[BufferLayout],
        attributes: &[VertexAttribute],
        shader: ShaderId,
        params: PipelineParams,
    ) -> Pipeline {
        unsafe {
            let shader_internal = &self.shaders[shader.0];

            let vertex_descriptor: ObjcId =
                msg_send![class!(MTLVertexDescriptor), vertexDescriptor];

            let attribute = |i, buffer_index, offset, format: MTLVertexFormat| {
                let mtl_attribute_desc = msg_send_![
                    msg_send_![vertex_descriptor, attributes],
                    objectAtIndexedSubscript: i
                ];
                msg_send_![mtl_attribute_desc, setBufferIndex: buffer_index];
                msg_send_![mtl_attribute_desc, setOffset: offset];
                msg_send_![mtl_attribute_desc, setFormat: format];
            };
            let layout = |i, step_func: VertexStep, stride, step_rate| {
                let mtl_buffer_desc = msg_send_![
                    msg_send_![vertex_descriptor, layouts],
                    objectAtIndexedSubscript: i
                ];
                let step_func: MTLVertexStepFunction = step_func.into();
                msg_send_![mtl_buffer_desc, setStride: stride];
                msg_send_![mtl_buffer_desc, setStepFunction: step_func];
                msg_send_![mtl_buffer_desc, setStepRate: step_rate];
            };

            let mut offsets = [0u64; 50];
            for (i, a) in attributes.iter().enumerate() {
                let offset = &mut offsets[a.buffer_index];
                attribute(
                    i as u64,
                    a.buffer_index as u64 + 1,
                    *offset,
                    a.format.into(),
                );
                *offset += a.format.size_bytes() as u64;
            }
            for (i, buffer) in buffer_layout.iter().enumerate() {
                layout(
                    i as u64 + 1,
                    buffer.step_func,
                    if buffer.stride == 0 {
                        offsets[i]
                    } else {
                        buffer.stride as u64
                    },
                    buffer.step_rate as u64,
                );
            }

            let descriptor = msg_send_![class!(MTLRenderPipelineDescriptor), new];
            msg_send_![descriptor, setVertexFunction:shader_internal.vertex_function];
            msg_send_![descriptor, setFragmentFunction:shader_internal.fragment_function];
            msg_send_![descriptor, setVertexDescriptor: vertex_descriptor];
            let color_attachments = msg_send_![descriptor, colorAttachments];
            for i in 0..2 {
                let color_attachment = msg_send_![color_attachments, objectAtIndexedSubscript: i];
                let view_pixel_format: MTLPixelFormat = msg_send![self.view, colorPixelFormat];
                msg_send_![color_attachment, setPixelFormat: view_pixel_format];
                if let Some(color_blend) = params.color_blend {
                    msg_send_![color_attachment, setBlendingEnabled: true];

                    let BlendState {
                        equation: eq_rgb,
                        sfactor: src_rgb,
                        dfactor: dst_rgb,
                    } = color_blend;
                    let BlendState {
                        equation: eq_alpha,
                        sfactor: src_alpha,
                        dfactor: dst_alpha,
                    } = color_blend;
                    msg_send_![
                        color_attachment,
                        setRgbBlendOperation: MTLBlendOperation::from(eq_rgb)
                    ];
                    msg_send_![
                        color_attachment,
                        setAlphaBlendOperation: MTLBlendOperation::from(eq_alpha)
                    ];
                    msg_send_![
                        color_attachment,
                        setSourceRGBBlendFactor: MTLBlendFactor::from(src_rgb)
                    ];
                    msg_send_![
                        color_attachment,
                        setSourceRGBBlendFactor: MTLBlendFactor::from(src_rgb)
                    ];
                    msg_send_![
                        color_attachment,
                        setSourceAlphaBlendFactor: MTLBlendFactor::from(src_alpha)
                    ];
                    msg_send_![
                        color_attachment,
                        setDestinationRGBBlendFactor: MTLBlendFactor::from(dst_rgb)
                    ];
                    msg_send_![
                        color_attachment,
                        setDestinationAlphaBlendFactor: MTLBlendFactor::from(dst_alpha)
                    ];
                }
            }
            // Tank Yard: attachment formats and sample count are set per pass
            // layout in `pipeline_state`.

            let depth_stencil_desc = msg_send_![class!(MTLDepthStencilDescriptor), new];
            msg_send_![depth_stencil_desc, setDepthWriteEnabled: BOOL::from(params.depth_write)];
            msg_send_![depth_stencil_desc, setDepthCompareFunction: MTLCompareFunction::from(params.depth_test)];

            // if let Some(stencil_test) = params.stencil_test {
            //     let back_face_stencil_desc = StencilDescriptor::new();
            //     back_face_stencil_desc.set_stencil_compare_function(stencil_test.back.test_func.into());
            //     back_face_stencil_desc.set_stencil_failure_operation(stencil_test.back.fail_op.into());
            //     back_face_stencil_desc
            //         .set_depth_failure_operation(stencil_test.back.depth_fail_op.into());
            //     back_face_stencil_desc.set_read_mask(stencil_test.back.test_mask);
            //     back_face_stencil_desc.set_write_mask(stencil_test.back.write_mask);

            //     depth_stencil_desc.set_back_face_stencil(Some(back_face_stencil_desc.as_ref()));

            //     let front_face_stencil_desc = StencilDescriptor::new();
            //     front_face_stencil_desc
            //         .set_stencil_compare_function(stencil_test.front.test_func.into());
            //     front_face_stencil_desc
            //         .set_stencil_failure_operation(stencil_test.front.fail_op.into());
            //     front_face_stencil_desc
            //         .set_depth_failure_operation(stencil_test.front.depth_fail_op.into());
            //     front_face_stencil_desc.set_read_mask(stencil_test.front.test_mask);
            //     front_face_stencil_desc.set_write_mask(stencil_test.front.write_mask);

            //     depth_stencil_desc.set_front_face_stencil(Some(front_face_stencil_desc.as_ref()))
            // }

            let depth_stencil_state = msg_send_![
                self.device,
                newDepthStencilStateWithDescriptor: depth_stencil_desc
            ];

            let pipeline = PipelineInternal {
                descriptor,
                states: Vec::new(),
                depth_stencil_state,
                //layout: buffer_layout.to_vec(),
                //attributes: vertex_layout,
                _shader: shader,
                //params,
            };

            self.pipelines.push(pipeline);

            Pipeline(self.pipelines.len() - 1)
        }
    }

    fn apply_pipeline(&mut self, pipeline: &Pipeline) {
        assert!(
            self.render_encoder.is_some(),
            "apply_pipeline before begin_pass"
        );
        let render_encoder = self.render_encoder.unwrap();

        unsafe {
            self.current_pipeline = Some(*pipeline);
            let state = self.pipeline_state(pipeline.0);
            let pipeline = &self.pipelines[pipeline.0];

            msg_send_![render_encoder, setRenderPipelineState: state];
            msg_send_![render_encoder, setDepthStencilState:pipeline.depth_stencil_state];
            // render_encoder.set_front_facing_winding(pipeline.params.front_face_order.into());
            // render_encoder.set_cull_mode(pipeline.params.cull_face.into());
        }
    }

    fn apply_bindings_from_slice(
        &mut self,
        vertex_buffers: &[BufferId],
        index_buffer: BufferId,
        textures: &[TextureId],
    ) {
        assert!(
            self.render_encoder.is_some(),
            "apply_bindings before begin_pass"
        );

        unsafe {
            let render_encoder = self.render_encoder.unwrap();
            for (index, vertex_buffer) in vertex_buffers.iter().enumerate() {
                let buffer = &mut self.buffers[vertex_buffer.0];
                let () = msg_send![render_encoder,
                                   setVertexBuffer:buffer.raw[buffer.value]
                                   offset:0
                                   atIndex:(index + 1) as u64];
                buffer.next_value = buffer.value + 1;
            }
            let index_buffer = &mut self.buffers[index_buffer.0];
            self.index_buffer = Some(index_buffer.raw[index_buffer.value]);
            index_buffer.next_value = index_buffer.value + 1;

            let img_count = textures.len();
            if img_count > 0 {
                for (n, img) in textures.iter().enumerate() {
                    let Texture {
                        sampler, texture, ..
                    } = self.textures.get(*img);
                    msg_send_![render_encoder, setFragmentSamplerState:sampler
                               atIndex:n
                    ];
                    msg_send_![render_encoder, setFragmentTexture:texture
                               atIndex:n
                    ];
                }
            }
        }
    }

    fn apply_uniforms_from_bytes(&mut self, uniform_ptr: *const u8, size: usize) {
        assert!(
            self.current_pipeline.is_some(),
            "apply_uniforms before apply_pipeline"
        );
        assert!(
            self.render_encoder.is_some(),
            "apply_uniforms before begin_pass"
        );

        let render_encoder = self.render_encoder.unwrap();

        self.current_frame_index = (self.current_frame_index + 1) % NUM_INFLIGHT_FRAMES;

        assert!(size < MAX_UNIFORM_BUFFER_SIZE as usize);

        assert!(self.current_ub_offset < MAX_UNIFORM_BUFFER_SIZE);

        let buffer = self.uniform_buffers[self.current_frame_index];
        unsafe {
            let dest: *mut std::ffi::c_void = msg_send![buffer, contents];
            std::ptr::copy(
                uniform_ptr as _,
                dest.add(self.current_ub_offset as usize),
                size,
            );

            #[cfg(target_os = "macos")]
            msg_send_![buffer, didModifyRange:NSRange::new(0, size as u64)];

            msg_send_![render_encoder,
                       setVertexBuffer:buffer
                       offset:self.current_ub_offset
                       atIndex:0];
            msg_send_![render_encoder,
                       setFragmentBuffer:buffer
                       offset:self.current_ub_offset
                       atIndex:0];
        }
        self.current_ub_offset = roundup_ub_buffer(self.current_ub_offset + size as u64);
    }

    fn begin_default_pass(&mut self, action: PassAction) {
        self.begin_pass(None, action)
    }

    fn begin_pass(&mut self, pass: Option<RenderPass>, action: PassAction) {
        let wanted = match pass {
            None => EncoderPass::Default,
            Some(pass) => EncoderPass::Offscreen(pass),
        };
        // Tank Yard: continue the open encoder when nothing needs clearing.
        if self.encoder_pass == Some(wanted) && matches!(action, PassAction::Nothing) {
            self.in_pass = true;
            return;
        }
        self.close_encoder();
        let command_buffer = self.command_buffer();
        unsafe {
            let (descriptor, target, resolve) = match pass {
                None => {
                    let samples: u64 = msg_send![self.view, sampleCount];
                    (
                        msg_send_![self.view, currentRenderPassDescriptor],
                        PassTarget {
                            format: msg_send![self.view, colorPixelFormat],
                            samples: samples.max(1),
                            depth: true,
                        },
                        samples > 1,
                    )
                }
                Some(pass) => {
                    let pass = &self.passes[pass.0];
                    (pass.render_pass_desc, pass.target, pass.resolve)
                }
            };
            assert!(!descriptor.is_null());

            let color_attachments = msg_send_![descriptor, colorAttachments];
            let color_attachment = msg_send_![color_attachments, objectAtIndexedSubscript: 0];
            let color_texture = msg_send_![color_attachment, texture];
            let width: u64 = msg_send![color_texture, width];
            let height: u64 = msg_send![color_texture, height];
            self.pass_size = (width.max(1) as u32, height.max(1) as u32);

            // Tank Yard: upstream always stored, so a multisampled window was
            // never resolved into the drawable. Keep the samples too: later
            // encoders this frame load them.
            let store = if resolve {
                MTLStoreAction::StoreAndMultisampleResolve
            } else {
                MTLStoreAction::Store
            };
            msg_send_![color_attachment, setStoreAction: store];

            match action {
                PassAction::Clear { color, .. } => {
                    msg_send_![color_attachment, setLoadAction: MTLLoadAction::Clear];

                    if let Some(color) = color {
                        msg_send_![color_attachment, setClearColor:MTLClearColor::new(color.0 as _, color.1 as _, color.2 as _, color.3 as _)];
                    }
                }
                PassAction::Nothing => {
                    msg_send_![color_attachment, setLoadAction: MTLLoadAction::Load];
                }
            }

            let render_encoder = msg_send_![
                command_buffer,
                renderCommandEncoderWithDescriptor: descriptor
            ];
            self.render_encoder = Some(render_encoder);
            self.encoder_pass = Some(wanted);
            self.pass_target = target;
            self.in_pass = true;
        }
    }

    fn end_render_pass(&mut self) {
        assert!(
            self.in_pass,
            "end_render_pass unpaired with begin_render_pass!"
        );
        // Tank Yard: the encoder stays open for the next pass into the same
        // target; `close_encoder` ends it when anything else needs the GPU.
        self.in_pass = false;
        self.index_buffer = None;
    }

    fn draw(&self, base_element: i32, num_elements: i32, num_instances: i32) {
        assert!(self.render_encoder.is_some(), "draw before begin_pass!");
        let render_encoder = self.render_encoder.unwrap();
        assert!(self.index_buffer.is_some());
        let index_buffer = self.index_buffer.unwrap();

        assert!(base_element == 0); // TODO: figure indexBufferOffset/baseVertex
        unsafe {
            msg_send_![render_encoder, drawIndexedPrimitives:MTLPrimitiveType::Triangle
                       indexCount:num_elements as u64
                       indexType:MTLIndexType::UInt16
                       indexBuffer:index_buffer
                       indexBufferOffset:0
                       instanceCount:num_instances as u64
                       baseVertex:0
                       baseInstance:0
            ];
        }
    }

    fn delete_shader(&mut self, _shader: ShaderId) {
        // TODO: place holder
    }
    fn delete_pipeline(&mut self, _pipeline: Pipeline) {
        // TODO: place holder
    }

    fn max_render_samples(&self) -> Option<i32> {
        // Tank Yard: the largest count this GPU supports for render targets.
        [8, 4, 2]
            .iter()
            .copied()
            .find(|&count| unsafe {
                let supported: BOOL =
                    msg_send![self.device, supportsTextureSampleCount: count as u64];
                supported == YES
            })
            .or(Some(1))
    }

    fn spatial_upscaler_available(&self) -> bool {
        self.metalfx.is_some()
    }

    fn spatial_upscale(&mut self, input: TextureId, output: TextureId) -> bool {
        let Some(class) = self.metalfx else {
            return false;
        };
        let (input, output) = (self.textures.get(input), self.textures.get(output));
        let key = (
            u64::from(input.params.width),
            u64::from(input.params.height),
            u64::from(output.params.width),
            u64::from(output.params.height),
            MTLPixelFormat::from(input.params.format),
            MTLPixelFormat::from(output.params.format),
        );
        if input.params.sample_count > 1 || output.params.sample_count > 1 {
            return false;
        }
        unsafe {
            if self.spatial.as_ref().map(|spatial| spatial.key) != Some(key) {
                self.spatial = None;
                let descriptor: ObjcId = msg_send![class, new];
                msg_send_![descriptor, setInputWidth: key.0];
                msg_send_![descriptor, setInputHeight: key.1];
                msg_send_![descriptor, setOutputWidth: key.2];
                msg_send_![descriptor, setOutputHeight: key.3];
                msg_send_![descriptor, setColorTextureFormat: key.4];
                msg_send_![descriptor, setOutputTextureFormat: key.5];
                // Perceptual: the input is display-ready sRGB-encoded color.
                msg_send_![descriptor, setColorProcessingMode: 0u64];
                let scaler: ObjcId =
                    msg_send![descriptor, newSpatialScalerWithDevice: self.device];
                msg_send_![descriptor, release];
                if scaler.is_null() {
                    // Unsupported layout: report it so callers fall back.
                    return false;
                }
                self.spatial = Some(SpatialScaler { scaler, key });
            }
            let scaler = self.spatial.as_ref().unwrap().scaler;
            self.close_encoder();
            let command_buffer = self.command_buffer();
            msg_send_![scaler, setColorTexture: input.texture];
            msg_send_![scaler, setOutputTexture: output.texture];
            msg_send_![scaler, setInputContentWidth: key.0];
            msg_send_![scaler, setInputContentHeight: key.1];
            msg_send_![scaler, encodeToCommandBuffer: command_buffer];
        }
        true
    }

    fn prepare_screen_reads(&mut self) {
        // Tank Yard: drawables become readable from the next frame on.
        unsafe { msg_send_![self.view, setFramebufferOnly: NO] };
    }

    fn read_screen(&mut self) -> Option<(u32, u32, Vec<u8>)> {
        self.close_encoder();
        unsafe {
            let drawable: ObjcId = msg_send![self.view, currentDrawable];
            if drawable.is_null() {
                return None;
            }
            let texture: ObjcId = msg_send![drawable, texture];
            let width: u64 = msg_send![texture, width];
            let height: u64 = msg_send![texture, height];
            let mut pixels = self.read_texture(texture, width as u32, height as u32);
            // Drawables are BGRA.
            for pixel in pixels.chunks_exact_mut(4) {
                pixel.swap(0, 2);
            }
            Some((width as u32, height as u32, pixels))
        }
    }

    fn commit_frame(&mut self) {
        self.close_encoder();
        unsafe {
            assert!(!self.command_queue.is_null());
            let command_buffer = self.command_buffer();
            let drawable: ObjcId = msg_send!(self.view, currentDrawable);
            // Tank Yard: no drawable (a zero-sized window) skips presenting.
            if !drawable.is_null() {
                msg_send_![command_buffer, presentDrawable: drawable];
            }
            msg_send_![command_buffer, commit];
            // Buffers rotate per frame, so the frame must finish before the
            // next one rewrites them.
            msg_send_![command_buffer, waitUntilCompleted];
        }
        for buffer in &mut self.buffers {
            buffer.next_value = 0;
        }
        self.current_ub_offset = 0;
        self.current_pipeline = None;
        self.command_buffer = None;
        if (self.current_frame_index + 1) >= 3 {
            self.current_frame_index = 0;
        }
    }
}
