use std::os::raw::{c_double, c_float, c_int, c_uint, c_void};

use imgui::{DrawCmd, DrawData, DrawVert, TextureId};
use vitasdk_sys::{sceDisplayWaitVblankStart, SCE_GXM_MULTISAMPLE_4X};

pub const SCREEN_W: u32 = 960;
pub const SCREEN_H: u32 = 544;

const GL_BLEND: c_uint = 0x0BE2;
const GL_COLOR_ARRAY: c_uint = 0x8076;
const GL_COLOR_BUFFER_BIT: c_uint = 0x0000_4000;
const GL_CULL_FACE: c_uint = 0x0B44;
const GL_DEPTH_TEST: c_uint = 0x0B71;
const GL_FLOAT: c_uint = 0x1406;
const GL_FRONT_AND_BACK: c_uint = 0x0408;
const GL_FILL: c_uint = 0x1B02;
const GL_LINEAR: c_int = 0x2601;
const GL_MODELVIEW: c_uint = 0x1700;
const GL_ONE_MINUS_SRC_ALPHA: c_uint = 0x0303;
const GL_POLYGON_MODE: c_uint = 0x0B40;
const GL_PROJECTION: c_uint = 0x1701;
const GL_RGBA: c_uint = 0x1908;
const GL_SCISSOR_BOX: c_uint = 0x0C10;
const GL_SCISSOR_TEST: c_uint = 0x0C11;
const GL_SRC_ALPHA: c_uint = 0x0302;
const GL_TEXTURE_2D: c_uint = 0x0DE1;
const GL_TEXTURE_BINDING_2D: c_uint = 0x8069;
const GL_TEXTURE_COORD_ARRAY: c_uint = 0x8078;
const GL_TEXTURE_MAG_FILTER: c_uint = 0x2800;
const GL_TEXTURE_MIN_FILTER: c_uint = 0x2801;
const GL_TRIANGLES: c_uint = 0x0004;
const GL_UNSIGNED_BYTE: c_uint = 0x1401;
const GL_VERTEX_ARRAY: c_uint = 0x8074;
const GL_VIEWPORT: c_uint = 0x0BA2;

extern "C" {
    fn vglInitExtended(
        legacy_pool_size: c_int,
        width: c_int,
        height: c_int,
        ram_threshold: c_int,
        msaa: c_int,
    ) -> u8;
    fn vglSwapBuffers(has_commondialog: u8);

    fn glBindTexture(target: c_uint, texture: c_uint);
    fn glBlendFunc(sfactor: c_uint, dfactor: c_uint);
    fn glClear(mask: c_uint);
    fn glClearColor(red: c_float, green: c_float, blue: c_float, alpha: c_float);
    fn glColorPointer(size: c_int, type_: c_uint, stride: c_int, pointer: *const c_void);
    fn glDeleteTextures(n: c_int, textures: *const c_uint);
    fn glDisable(cap: c_uint);
    fn glDisableClientState(array: c_uint);
    fn glDrawArrays(mode: c_uint, first: c_int, count: c_int);
    fn glEnable(cap: c_uint);
    fn glEnableClientState(array: c_uint);
    fn glGenTextures(n: c_int, textures: *mut c_uint);
    fn glGetIntegerv(pname: c_uint, data: *mut c_int);
    fn glLoadIdentity();
    fn glMatrixMode(mode: c_uint);
    fn glOrtho(
        left: c_double,
        right: c_double,
        bottom: c_double,
        top: c_double,
        near: c_double,
        far: c_double,
    );
    fn glPolygonMode(face: c_uint, mode: c_uint);
    fn glPopMatrix();
    fn glPushMatrix();
    fn glScissor(x: c_int, y: c_int, width: c_int, height: c_int);
    fn glTexCoordPointer(size: c_int, type_: c_uint, stride: c_int, pointer: *const c_void);
    fn glTexImage2D(
        target: c_uint,
        level: c_int,
        internalformat: c_int,
        width: c_int,
        height: c_int,
        border: c_int,
        format: c_uint,
        type_: c_uint,
        pixels: *const c_void,
    );
    fn glTexParameteri(target: c_uint, pname: c_uint, param: c_int);
    fn glVertexPointer(size: c_int, type_: c_uint, stride: c_int, pointer: *const c_void);
    fn glViewport(x: c_int, y: c_int, width: c_int, height: c_int);
}

pub fn init_vitagl() {
    unsafe {
        // vglInitExtended returns whether the requested resolution had to fall
        // back, not whether initialization succeeded. `false` is expected for
        // a normal 960x544 init.
        let resolution_fallback = vglInitExtended(
            0,
            SCREEN_W as c_int,
            SCREEN_H as c_int,
            0x0180_0000,
            SCE_GXM_MULTISAMPLE_4X as c_int,
        ) != 0;
        eprintln!("mpvrs: vitaGL initialized, resolution_fallback={resolution_fallback}");
    }
}

pub fn clear_screen() {
    unsafe {
        glClearColor(0.05, 0.06, 0.08, 1.0);
        glClear(GL_COLOR_BUFFER_BIT);
    }
}

pub fn present() {
    unsafe {
        vglSwapBuffers(0);
        sceDisplayWaitVblankStart();
    }
}

#[derive(Clone, Copy, Debug)]
pub struct GlTexture {
    pub gl_id: u32,
    pub texture_id: TextureId,
    pub bytes: usize,
}

pub fn create_rgba_texture(width: u32, height: u32, pixels: &[u8]) -> Option<GlTexture> {
    let expected_len = width.checked_mul(height)?.checked_mul(4)? as usize;
    if width == 0 || height == 0 || pixels.len() != expected_len {
        return None;
    }

    let mut texture = 0;
    unsafe {
        let mut last_texture = 0;
        glGetIntegerv(GL_TEXTURE_BINDING_2D, &mut last_texture);
        glGenTextures(1, &mut texture);
        if texture == 0 {
            glBindTexture(GL_TEXTURE_2D, last_texture as c_uint);
            return None;
        }
        glBindTexture(GL_TEXTURE_2D, texture);
        glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MIN_FILTER, GL_LINEAR);
        glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MAG_FILTER, GL_LINEAR);
        glTexImage2D(
            GL_TEXTURE_2D,
            0,
            GL_RGBA as c_int,
            width as c_int,
            height as c_int,
            0,
            GL_RGBA,
            GL_UNSIGNED_BYTE,
            pixels.as_ptr().cast(),
        );
        glBindTexture(GL_TEXTURE_2D, last_texture as c_uint);
    }

    Some(GlTexture {
        gl_id: texture,
        texture_id: TextureId::new(texture as usize),
        bytes: expected_len,
    })
}

pub fn delete_texture(gl_id: u32) {
    if gl_id == 0 {
        return;
    }
    unsafe {
        glDeleteTextures(1, &gl_id);
    }
}

pub struct VitaGlImguiRenderer {
    font_texture: c_uint,
    vertices: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
    colors: Vec<[u8; 4]>,
}

impl VitaGlImguiRenderer {
    pub fn new(imgui: &mut imgui::Context) -> Self {
        let (width, height, pixels) = {
            let texture = imgui.fonts().build_rgba32_texture();
            (texture.width, texture.height, texture.data.to_vec())
        };

        let mut font_texture = 0;
        unsafe {
            let mut last_texture = 0;
            glGetIntegerv(GL_TEXTURE_BINDING_2D, &mut last_texture);
            glGenTextures(1, &mut font_texture);
            glBindTexture(GL_TEXTURE_2D, font_texture);
            glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MIN_FILTER, GL_LINEAR);
            glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MAG_FILTER, GL_LINEAR);
            glTexImage2D(
                GL_TEXTURE_2D,
                0,
                GL_RGBA as c_int,
                width as c_int,
                height as c_int,
                0,
                GL_RGBA,
                GL_UNSIGNED_BYTE,
                pixels.as_ptr().cast(),
            );
            glBindTexture(GL_TEXTURE_2D, last_texture as c_uint);
        }

        imgui.fonts().tex_id = TextureId::new(font_texture as usize);

        Self {
            font_texture,
            vertices: Vec::with_capacity(8192),
            uvs: Vec::with_capacity(8192),
            colors: Vec::with_capacity(8192),
        }
    }

    pub fn render(&mut self, draw_data: &DrawData) {
        let fb_width = (draw_data.display_size[0] * draw_data.framebuffer_scale[0]) as c_int;
        let fb_height = (draw_data.display_size[1] * draw_data.framebuffer_scale[1]) as c_int;
        if fb_width <= 0 || fb_height <= 0 {
            return;
        }

        let mut last_texture = 0;
        let mut last_polygon_mode = [0; 2];
        let mut last_viewport = [0; 4];
        let mut last_scissor_box = [0; 4];

        unsafe {
            glGetIntegerv(GL_TEXTURE_BINDING_2D, &mut last_texture);
            glGetIntegerv(GL_POLYGON_MODE, last_polygon_mode.as_mut_ptr());
            glGetIntegerv(GL_VIEWPORT, last_viewport.as_mut_ptr());
            glGetIntegerv(GL_SCISSOR_BOX, last_scissor_box.as_mut_ptr());

            glEnable(GL_BLEND);
            glBlendFunc(GL_SRC_ALPHA, GL_ONE_MINUS_SRC_ALPHA);
            glDisable(GL_CULL_FACE);
            glDisable(GL_DEPTH_TEST);
            glEnable(GL_SCISSOR_TEST);
            glEnableClientState(GL_VERTEX_ARRAY);
            glEnableClientState(GL_TEXTURE_COORD_ARRAY);
            glEnableClientState(GL_COLOR_ARRAY);
            glEnable(GL_TEXTURE_2D);
            glPolygonMode(GL_FRONT_AND_BACK, GL_FILL);

            glViewport(0, 0, fb_width, fb_height);
            glMatrixMode(GL_PROJECTION);
            glPushMatrix();
            glLoadIdentity();
            glOrtho(
                0.0,
                draw_data.display_size[0] as c_double,
                draw_data.display_size[1] as c_double,
                0.0,
                0.0,
                1.0,
            );
            glMatrixMode(GL_MODELVIEW);
            glPushMatrix();
            glLoadIdentity();
        }

        for draw_list in draw_data.draw_lists() {
            let idx_buffer = draw_list.idx_buffer();
            let vtx_buffer = draw_list.vtx_buffer();

            for command in draw_list.commands() {
                let DrawCmd::Elements { count, cmd_params } = command else {
                    continue;
                };

                self.vertices.clear();
                self.uvs.clear();
                self.colors.clear();
                self.vertices.reserve(count);
                self.uvs.reserve(count);
                self.colors.reserve(count);

                for i in 0..count {
                    let idx =
                        idx_buffer[cmd_params.idx_offset + i] as usize + cmd_params.vtx_offset;
                    let DrawVert { pos, uv, col } = vtx_buffer[idx];
                    self.vertices.push([pos[0], pos[1], 0.0]);
                    self.uvs.push(uv);
                    self.colors.push(col);
                }

                let clip = cmd_params.clip_rect;
                let clip_x = clip[0].max(0.0) as c_int;
                let clip_y = (fb_height as f32 - clip[3]).max(0.0) as c_int;
                let clip_w = (clip[2] - clip[0]).max(0.0) as c_int;
                let clip_h = (clip[3] - clip[1]).max(0.0) as c_int;

                unsafe {
                    glBindTexture(GL_TEXTURE_2D, cmd_params.texture_id.id() as c_uint);
                    glScissor(clip_x, clip_y, clip_w, clip_h);
                    glVertexPointer(3, GL_FLOAT, 0, self.vertices.as_ptr().cast());
                    glTexCoordPointer(2, GL_FLOAT, 0, self.uvs.as_ptr().cast());
                    glColorPointer(4, GL_UNSIGNED_BYTE, 0, self.colors.as_ptr().cast());
                    glDrawArrays(GL_TRIANGLES, 0, count as c_int);
                }
            }
        }

        unsafe {
            glDisableClientState(GL_COLOR_ARRAY);
            glDisableClientState(GL_TEXTURE_COORD_ARRAY);
            glDisableClientState(GL_VERTEX_ARRAY);
            glBindTexture(GL_TEXTURE_2D, last_texture as c_uint);
            glMatrixMode(GL_MODELVIEW);
            glPopMatrix();
            glMatrixMode(GL_PROJECTION);
            glPopMatrix();
            glPolygonMode(GL_FRONT_AND_BACK, last_polygon_mode[0] as c_uint);
            glViewport(
                last_viewport[0],
                last_viewport[1],
                last_viewport[2],
                last_viewport[3],
            );
            glScissor(
                last_scissor_box[0],
                last_scissor_box[1],
                last_scissor_box[2],
                last_scissor_box[3],
            );
        }
    }
}

impl Drop for VitaGlImguiRenderer {
    fn drop(&mut self) {
        unsafe {
            glDeleteTextures(1, &self.font_texture);
        }
    }
}
