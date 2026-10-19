//! Tiny OpenGL loader: GL 1.1 comes from opengl32 via windows-sys, everything newer via wglGetProcAddress.

use std::ffi::CString;
use windows_sys::Win32::Graphics::OpenGL::wglGetProcAddress;
use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleA, GetProcAddress, LoadLibraryA};

pub const FRAGMENT_SHADER: u32 = 0x8B30;
pub const VERTEX_SHADER: u32 = 0x8B31;
pub const COMPILE_STATUS: u32 = 0x8B81;
pub const LINK_STATUS: u32 = 0x8B82;
pub const FRAMEBUFFER: u32 = 0x8D40;
pub const COLOR_ATTACHMENT0: u32 = 0x8CE0;
pub const FRAMEBUFFER_COMPLETE: u32 = 0x8CD5;
pub const TEXTURE0: u32 = 0x84C0;
pub const RGBA16F: u32 = 0x881A;
pub const HALF_FLOAT: u32 = 0x140B;
pub const LUMINANCE8: u32 = 0x8040;
pub const CLAMP_TO_EDGE: i32 = 0x812F;
pub const LINEAR_MIPMAP_LINEAR: i32 = 0x2703;

unsafe fn proc_addr(name: &str) -> *const () {
    let c = CString::new(name).unwrap();
    if let Some(p) = wglGetProcAddress(c.as_ptr() as *const u8) {
        let v = p as usize;
        if v > 3 && v != usize::MAX {
            return p as *const ();
        }
    }
    let lib = GetModuleHandleA(b"opengl32.dll\0".as_ptr());
    let lib = if lib.is_null() { LoadLibraryA(b"opengl32.dll\0".as_ptr()) } else { lib };
    GetProcAddress(lib, c.as_ptr() as *const u8).map_or(std::ptr::null(), |p| p as *const ())
}

macro_rules! gl_fns {
    ($($name:ident($($a:ty),*) $(-> $r:ty)?;)*) => {
        #[allow(non_snake_case)]
        pub struct Gl { $(pub $name: unsafe extern "system" fn($($a),*) $(-> $r)?,)* }
        impl Gl {
            /// A GL context must be current on this thread.
            pub unsafe fn load() -> Result<Gl, String> {
                Ok(Gl { $($name: {
                    let p = proc_addr(concat!("gl", stringify!($name)));
                    if p.is_null() { return Err(concat!("missing OpenGL function gl", stringify!($name)).to_string()); }
                    std::mem::transmute::<*const (), unsafe extern "system" fn($($a),*) $(-> $r)?>(p)
                },)* })
            }
        }
    };
}

gl_fns! {
    CreateShader(u32) -> u32;
    ShaderSource(u32, i32, *const *const i8, *const i32);
    CompileShader(u32);
    GetShaderiv(u32, u32, *mut i32);
    GetShaderInfoLog(u32, i32, *mut i32, *mut i8);
    CreateProgram() -> u32;
    AttachShader(u32, u32);
    LinkProgram(u32);
    GetProgramiv(u32, u32, *mut i32);
    GetProgramInfoLog(u32, i32, *mut i32, *mut i8);
    UseProgram(u32);
    DeleteShader(u32);
    DeleteProgram(u32);
    GetUniformLocation(u32, *const i8) -> i32;
    Uniform1f(i32, f32);
    Uniform2f(i32, f32, f32);
    Uniform1i(i32, i32);
    GenFramebuffers(i32, *mut u32);
    BindFramebuffer(u32, u32);
    FramebufferTexture2D(u32, u32, u32, u32, i32);
    CheckFramebufferStatus(u32) -> u32;
    GenerateMipmap(u32);
    ActiveTexture(u32);
}

const VS: &str = "#version 120\nvarying vec2 uv;\nvoid main(){ gl_Position = gl_Vertex; uv = gl_MultiTexCoord0.xy; }\n";

unsafe fn compile(gl: &Gl, kind: u32, src: &str) -> Result<u32, String> {
    let sh = (gl.CreateShader)(kind);
    let c = CString::new(src).unwrap();
    let p = c.as_ptr();
    (gl.ShaderSource)(sh, 1, &p, std::ptr::null());
    (gl.CompileShader)(sh);
    let mut ok = 0;
    (gl.GetShaderiv)(sh, COMPILE_STATUS, &mut ok);
    if ok == 0 {
        let mut buf = vec![0i8; 4096];
        let mut n = 0;
        (gl.GetShaderInfoLog)(sh, 4096, &mut n, buf.as_mut_ptr());
        let log: Vec<u8> = buf[..n.max(0) as usize].iter().map(|&b| b as u8).collect();
        return Err(format!("shader compile failed: {}", String::from_utf8_lossy(&log)));
    }
    Ok(sh)
}

/// Compile + link a program with the shared full-quad vertex shader.
pub unsafe fn program(gl: &Gl, fragment: &str) -> Result<u32, String> {
    let vs = compile(gl, VERTEX_SHADER, VS)?;
    let fs = compile(gl, FRAGMENT_SHADER, fragment)?;
    let prog = (gl.CreateProgram)();
    (gl.AttachShader)(prog, vs);
    (gl.AttachShader)(prog, fs);
    (gl.LinkProgram)(prog);
    (gl.DeleteShader)(vs);
    (gl.DeleteShader)(fs);
    let mut ok = 0;
    (gl.GetProgramiv)(prog, LINK_STATUS, &mut ok);
    if ok == 0 {
        let mut buf = vec![0i8; 4096];
        let mut n = 0;
        (gl.GetProgramInfoLog)(prog, 4096, &mut n, buf.as_mut_ptr());
        let log: Vec<u8> = buf[..n.max(0) as usize].iter().map(|&b| b as u8).collect();
        return Err(format!("program link failed: {}", String::from_utf8_lossy(&log)));
    }
    Ok(prog)
}

pub unsafe fn uloc(gl: &Gl, prog: u32, name: &str) -> i32 {
    let c = CString::new(name).unwrap();
    (gl.GetUniformLocation)(prog, c.as_ptr())
}
