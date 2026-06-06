#[link(name = "vitaGL", kind = "static")]
#[link(name = "vitashark", kind = "static")]
#[link(name = "SceShaccCg_stub", kind = "static")]
#[link(name = "mathneon", kind = "static")]
#[link(name = "SceShaccCgExt", kind = "static")]
#[link(name = "taihen_stub", kind = "static")]
extern "C" {}

#[no_mangle]
pub static mut _newlib_heap_size_user: i32 = 64 * 1024 * 1024;

use std::backtrace::Backtrace;
use std::fs;
use std::os::raw::{c_double, c_float, c_int, c_uint, c_void};
use std::panic;
use std::path::Path;

use std::thread;
use std::time::{Duration, Instant};

use imgui::{
    BackendFlags, Condition, ConfigFlags, DrawCmd, DrawData, DrawVert, Key, NavInput, TextureId, Ui,
};
use vitasdk_sys::{
    sceCtrlPeekBufferPositive, sceCtrlSetSamplingMode, sceDisplayWaitVblankStart, sceTouchPeek,
    sceTouchSetSamplingState, SceCtrlData, SceTouchData, SCE_CTRL_CIRCLE, SCE_CTRL_CROSS,
    SCE_CTRL_DOWN, SCE_CTRL_LEFT, SCE_CTRL_LTRIGGER, SCE_CTRL_RIGHT, SCE_CTRL_RTRIGGER,
    SCE_CTRL_SELECT, SCE_CTRL_SQUARE, SCE_CTRL_TRIANGLE, SCE_CTRL_UP, SCE_GXM_MULTISAMPLE_4X,
    SCE_TOUCH_PORT_FRONT, SCE_TOUCH_SAMPLING_STATE_START,
};

const SCREEN_W: u32 = 960;
const SCREEN_H: u32 = 544;
const ROOT_PATH: &str = "ux0:/";
const VITA_TOUCH_W: f32 = 1919.0;
const VITA_TOUCH_H: f32 = 1087.0;

// Dear ImGui's legacy `io.KeyMap` maps ImGui keys to indices in
// `io.KeysDown`, so values must be small key indices (< 512), not platform
// button bitmasks like `SCE_CTRL_UP`.
const VITA_IMGUI_KEY_UP: u32 = 256;
const VITA_IMGUI_KEY_DOWN: u32 = 257;
const VITA_IMGUI_KEY_LEFT: u32 = 258;
const VITA_IMGUI_KEY_RIGHT: u32 = 259;
const VITA_IMGUI_KEY_CROSS: u32 = 260;
const VITA_IMGUI_KEY_CIRCLE: u32 = 261;

const GL_BLEND: c_uint = 0x0BE2;
const GL_COLOR_ARRAY: c_uint = 0x8076;
const GL_COLOR_BUFFER_BIT: c_uint = 0x0000_4000;
const GL_CULL_FACE: c_uint = 0x0B44;
const GL_DEPTH_TEST: c_uint = 0x0B71;

const GL_FRONT_AND_BACK: c_uint = 0x0408;
const GL_FILL: c_uint = 0x1B02;
const GL_FLOAT: c_uint = 0x1406;
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
const GL_UNSIGNED_SHORT: c_uint = 0x1403;
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
    fn glDeleteTextures(n: c_int, textures: *const c_uint);
    fn glDisable(cap: c_uint);
    fn glDisableClientState(array: c_uint);
    fn glEnable(cap: c_uint);
    fn glEnableClientState(array: c_uint);
    fn glColorPointer(size: c_int, type_: c_uint, stride: c_int, pointer: *const c_void);
    fn glDrawArrays(mode: c_uint, first: c_int, count: c_int);
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
    fn glPopMatrix();
    fn glPushMatrix();
    fn glScissor(x: c_int, y: c_int, width: c_int, height: c_int);
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
    fn glTexCoordPointer(size: c_int, type_: c_uint, stride: c_int, pointer: *const c_void);
    fn glTexParameteri(target: c_uint, pname: c_uint, param: c_int);
    fn glVertexPointer(size: c_int, type_: c_uint, stride: c_int, pointer: *const c_void);
    fn glViewport(x: c_int, y: c_int, width: c_int, height: c_int);
    fn glPolygonMode(face: c_uint, mode: c_uint);
}

#[derive(Clone, Debug)]
struct FileEntry {
    name: String,
    path: String,
    is_dir: bool,
}

struct AppState {
    entries: Vec<FileEntry>,
    selected: Option<usize>,
    status: String,
    last_refresh: Instant,
}

#[derive(Clone, Copy, Debug, Default)]
struct RenderStats {
    fb_width: c_int,
    fb_height: c_int,
    draw_lists: usize,
    commands: usize,
    skipped_commands: usize,
    elements: usize,
    vertices: usize,
    first_texture_id: usize,
    first_clip_rect: Option<[f32; 4]>,
}

struct VitaGlImguiRenderer {
    font_texture: c_uint,
    vertices: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
    colors: Vec<[u8; 4]>,
    logged_first_geometry: bool,
}

impl VitaGlImguiRenderer {
    fn new(imgui: &mut imgui::Context) -> Self {
        let (width, height, pixels) = {
            let texture = imgui.fonts().build_rgba32_texture();
            (texture.width, texture.height, texture.data.to_vec())
        };

        eprintln!(
            "mpvrs: built imgui font atlas: {width}x{height}, {} RGBA bytes",
            pixels.len()
        );

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
        eprintln!("mpvrs: uploaded font texture id={font_texture}");

        Self {
            font_texture,
            vertices: Vec::with_capacity(8192),
            uvs: Vec::with_capacity(8192),
            colors: Vec::with_capacity(8192),
            logged_first_geometry: false,
        }
    }

    fn render(&mut self, draw_data: &DrawData) -> RenderStats {
        let fb_width = (draw_data.display_size[0] * draw_data.framebuffer_scale[0]) as c_int;
        let fb_height = (draw_data.display_size[1] * draw_data.framebuffer_scale[1]) as c_int;
        let mut stats = RenderStats {
            fb_width,
            fb_height,
            ..RenderStats::default()
        };
        if fb_width <= 0 || fb_height <= 0 {
            return stats;
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
            stats.draw_lists += 1;
            let idx_buffer = draw_list.idx_buffer();
            let vtx_buffer = draw_list.vtx_buffer();
            stats.vertices += vtx_buffer.len();

            for command in draw_list.commands() {
                let DrawCmd::Elements { count, cmd_params } = command else {
                    stats.skipped_commands += 1;
                    continue;
                };
                stats.commands += 1;
                stats.elements += count;
                if stats.first_texture_id == 0 {
                    stats.first_texture_id = cmd_params.texture_id.id();
                    stats.first_clip_rect = Some(cmd_params.clip_rect);
                }

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

                if !self.logged_first_geometry {
                    self.logged_first_geometry = true;
                    let mut min = [f32::INFINITY; 2];
                    let mut max = [f32::NEG_INFINITY; 2];
                    for vertex in &self.vertices {
                        min[0] = min[0].min(vertex[0]);
                        min[1] = min[1].min(vertex[1]);
                        max[0] = max[0].max(vertex[0]);
                        max[1] = max[1].max(vertex[1]);
                    }
                    eprintln!(
                        "mpvrs: first imgui draw cmd: count={count}, idx_offset={}, vtx_offset={}, idx_buffer={}, vtx_buffer={}, pos_min={min:?}, pos_max={max:?}",
                        cmd_params.idx_offset,
                        cmd_params.vtx_offset,
                        idx_buffer.len(),
                        vtx_buffer.len(),
                    );
                    for i in 0..self.vertices.len().min(6) {
                        eprintln!(
                            "mpvrs: first imgui vertex[{i}]: pos={:?}, uv={:?}, color_rgba={:?}",
                            self.vertices[i], self.uvs[i], self.colors[i]
                        );
                    }
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

        stats
    }
}

impl Drop for VitaGlImguiRenderer {
    fn drop(&mut self) {
        unsafe {
            glDeleteTextures(1, &self.font_texture);
        }
    }
}

impl AppState {
    fn new() -> Self {
        let mut app = Self {
            entries: Vec::new(),
            selected: None,
            status: String::new(),
            last_refresh: Instant::now(),
        };
        app.refresh();
        app
    }

    fn refresh(&mut self) {
        match read_one_level(ROOT_PATH) {
            Ok(entries) => {
                eprintln!(
                    "mpvrs: read {ROOT_PATH}: {} entries; first={:?}",
                    entries.len(),
                    entries.first().map(|entry| (&entry.name, entry.is_dir))
                );
                self.entries = entries;
                self.selected = self.selected.filter(|&idx| idx < self.entries.len());
                self.status = format!("{} entries in {}", self.entries.len(), ROOT_PATH);
            }
            Err(err) => {
                eprintln!("mpvrs: failed to read {ROOT_PATH}: {err}");
                self.entries.clear();
                self.selected = None;
                self.status = format!("Failed to read {ROOT_PATH}: {err}");
            }
        }
        self.last_refresh = Instant::now();
    }

    fn selected_entry(&self) -> Option<&FileEntry> {
        self.selected.and_then(|idx| self.entries.get(idx))
    }
}

fn main() {
    install_panic_hook();

    unsafe {
        sceCtrlSetSamplingMode(1);
        sceTouchSetSamplingState(SCE_TOUCH_PORT_FRONT, SCE_TOUCH_SAMPLING_STATE_START);

        // vglInitExtended returns whether the requested resolution had to fall
        // back, not whether initialization succeeded. `false` is the expected
        // value for a normal 960x544 init.
        let resolution_fallback = vglInitExtended(
            0,
            SCREEN_W as c_int,
            SCREEN_H as c_int,
            0x0180_0000,
            SCE_GXM_MULTISAMPLE_4X as c_int,
        ) != 0;
        eprintln!("mpvrs: vitaGL initialized, resolution_fallback={resolution_fallback}");
    }

    let mut imgui = imgui::Context::create();
    imgui.set_ini_filename(None);
    eprintln!("mpvrs: imgui context created");
    {
        let io = imgui.io_mut();
        io.display_size = [SCREEN_W as f32, SCREEN_H as f32];
        io.config_flags |= ConfigFlags::NAV_ENABLE_KEYBOARD;
        io.config_flags |= ConfigFlags::NAV_ENABLE_GAMEPAD;
        io.config_flags |= ConfigFlags::IS_TOUCH_SCREEN;
        io.backend_flags |= BackendFlags::HAS_GAMEPAD;

        io.key_map[Key::UpArrow as usize] = VITA_IMGUI_KEY_UP;
        io.key_map[Key::DownArrow as usize] = VITA_IMGUI_KEY_DOWN;
        io.key_map[Key::LeftArrow as usize] = VITA_IMGUI_KEY_LEFT;
        io.key_map[Key::RightArrow as usize] = VITA_IMGUI_KEY_RIGHT;
        io.key_map[Key::Enter as usize] = VITA_IMGUI_KEY_CROSS;
        io.key_map[Key::Escape as usize] = VITA_IMGUI_KEY_CIRCLE;
        io.key_map[Key::Space as usize] = VITA_IMGUI_KEY_CROSS;

        eprintln!(
            "mpvrs: imgui io initialized: display={:?}, keys_down={}, nav_inputs={}, key_map(up/down/left/right/enter/escape/space)={}/{}/{}/{}/{}/{}/{}",
            io.display_size,
            io.keys_down.len(),
            io.nav_inputs.len(),
            io.key_map[Key::UpArrow as usize],
            io.key_map[Key::DownArrow as usize],
            io.key_map[Key::LeftArrow as usize],
            io.key_map[Key::RightArrow as usize],
            io.key_map[Key::Enter as usize],
            io.key_map[Key::Escape as usize],
            io.key_map[Key::Space as usize],
        );
    }

    let mut renderer = VitaGlImguiRenderer::new(&mut imgui);

    let mut app = AppState::new();
    let mut last_frame = Instant::now();
    let mut frame_no: u64 = 0;
    let mut last_buttons = 0;
    let mut last_touch_down = false;

    loop {
        frame_no += 1;
        let ctrl = read_ctrl();
        if pressed(&ctrl, SCE_CTRL_SELECT) {
            break;
        }
        if pressed(&ctrl, SCE_CTRL_TRIANGLE) {
            app.refresh();
        }

        if ctrl.buttons != last_buttons {
            eprintln!(
                "mpvrs: frame {frame_no}: buttons changed {last_buttons:#x} -> {:#x}",
                ctrl.buttons
            );
            last_buttons = ctrl.buttons;
        }

        let touch = read_front_touch();
        let touch_down = touch.is_some();
        if touch_down != last_touch_down {
            eprintln!("mpvrs: frame {frame_no}: touch_down={touch_down}, pos={touch:?}");
            last_touch_down = touch_down;
        }

        let now = Instant::now();
        let delta = now.saturating_duration_since(last_frame);
        last_frame = now;

        {
            let io = imgui.io_mut();
            io.update_delta_time(delta);
            io.display_size = [SCREEN_W as f32, SCREEN_H as f32];
            apply_controller_to_imgui(io, &ctrl);
            apply_touch_to_imgui(io, touch);
        }

        let ui = imgui.frame();
        draw_ui(ui, &mut app);

        if should_log_frame(frame_no) {
            let io = imgui.io();
            eprintln!(
                "mpvrs: frame {frame_no}: before render dt={:.4}s, display={:?}, mouse_pos={:?}, mouse_down={:?}, want_mouse={}, want_keyboard={}, nav_active={}, entries={}, status={:?}",
                delta.as_secs_f32(),
                io.display_size,
                io.mouse_pos,
                io.mouse_down,
                io.want_capture_mouse,
                io.want_capture_keyboard,
                io.nav_active,
                app.entries.len(),
                app.status,
            );
        }

        unsafe {
            glClearColor(0.05, 0.06, 0.08, 1.0);
            glClear(GL_COLOR_BUFFER_BIT);
        }
        draw_debug_vgl_probe();

        let draw_data = imgui.render();
        let stats = renderer.render(draw_data);
        if should_log_frame(frame_no) {
            eprintln!(
                "mpvrs: frame {frame_no}: render stats: fb={}x{}, lists={}, cmds={}, skipped={}, elems={}, vtx={}, first_tex={}, first_clip={:?}",
                stats.fb_width,
                stats.fb_height,
                stats.draw_lists,
                stats.commands,
                stats.skipped_commands,
                stats.elements,
                stats.vertices,
                stats.first_texture_id,
                stats.first_clip_rect,
            );
        }

        unsafe {
            vglSwapBuffers(0);
            sceDisplayWaitVblankStart();
        }
    }

    drop(renderer);
}

fn draw_debug_vgl_probe() {
    let vertices: [[f32; 3]; 6] = [
        [24.0, 24.0, 0.0],
        [360.0, 24.0, 0.0],
        [360.0, 120.0, 0.0],
        [24.0, 24.0, 0.0],
        [360.0, 120.0, 0.0],
        [24.0, 120.0, 0.0],
    ];
    let colors: [[u8; 4]; 6] = [
        [255, 0, 255, 255],
        [0, 255, 255, 255],
        [255, 255, 0, 255],
        [255, 0, 255, 255],
        [255, 255, 0, 255],
        [255, 255, 255, 255],
    ];
    unsafe {
        glDisable(GL_BLEND);
        glDisable(GL_CULL_FACE);
        glDisable(GL_DEPTH_TEST);
        glDisable(GL_SCISSOR_TEST);
        glDisable(GL_TEXTURE_2D);
        glEnableClientState(GL_VERTEX_ARRAY);
        glDisableClientState(GL_TEXTURE_COORD_ARRAY);
        glEnableClientState(GL_COLOR_ARRAY);

        glViewport(0, 0, SCREEN_W as c_int, SCREEN_H as c_int);
        glMatrixMode(GL_PROJECTION);
        glPushMatrix();
        glLoadIdentity();
        glOrtho(
            0.0,
            SCREEN_W as c_double,
            SCREEN_H as c_double,
            0.0,
            0.0,
            1.0,
        );
        glMatrixMode(GL_MODELVIEW);
        glPushMatrix();
        glLoadIdentity();

        glVertexPointer(3, GL_FLOAT, 0, vertices.as_ptr().cast());
        glColorPointer(4, GL_UNSIGNED_BYTE, 0, colors.as_ptr().cast());
        glDrawArrays(GL_TRIANGLES, 0, vertices.len() as c_int);

        glMatrixMode(GL_MODELVIEW);
        glPopMatrix();
        glMatrixMode(GL_PROJECTION);
        glPopMatrix();
        glEnable(GL_TEXTURE_2D);
    }
}

fn should_log_frame(frame_no: u64) -> bool {
    frame_no <= 10 || frame_no % 60 == 0
}

fn install_panic_hook() {
    panic::set_hook(Box::new(|info| {
        eprintln!("mpvrs panic: {info}");
        eprintln!("{}", Backtrace::force_capture());
        thread::sleep(Duration::from_secs(5));
    }));
}

fn read_one_level(root: &str) -> Result<Vec<FileEntry>, std::io::Error> {
    let mut entries = Vec::new();

    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;
        let name = entry.file_name().to_string_lossy().into_owned();

        entries.push(FileEntry {
            name,
            path: path_to_string(&path),
            is_dir: file_type.is_dir(),
        });
    }

    entries.sort_by(|a, b| match (a.is_dir, b.is_dir) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
    });

    Ok(entries)
}

fn path_to_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn draw_ui(ui: &Ui, app: &mut AppState) {
    ui.window("mpvrs")
        .position([0.0, 0.0], Condition::Always)
        .size([SCREEN_W as f32, SCREEN_H as f32], Condition::Always)
        .movable(false)
        .resizable(false)
        .collapsible(false)
        .build(|| {
            ui.text("ux0:/ one-level browser");
            ui.same_line();
            if ui.button("Refresh (Triangle)") {
                app.refresh();
            }

            ui.separator();
            ui.text(&app.status);

            if let Some(selected) = app.selected_entry() {
                let kind = if selected.is_dir { "folder" } else { "file" };
                ui.text(format!("Selected {kind}: {}", selected.path));
            } else {
                ui.text("Select with touch, d-pad + cross, or keyboard navigation.");
            }

            ui.text("Select exits for now. Later: Cross=open/play, Circle=back.");
            ui.separator();

            let list_height = SCREEN_H as f32 - 165.0;
            ui.child_window("files")
                .size([0.0, list_height])
                .border(true)
                .build(|| {
                    for (idx, entry) in app.entries.iter().enumerate() {
                        let prefix = if entry.is_dir { "[DIR]" } else { "     " };
                        let label = format!("{prefix} {}##{}", entry.name, idx);
                        let selected = app.selected == Some(idx);

                        if ui.selectable_config(&label).selected(selected).build() {
                            app.selected = Some(idx);
                            app.status = format!("Selected {}", entry.path);
                        }
                    }
                });
        });
}

fn read_ctrl() -> SceCtrlData {
    let mut ctrl = unsafe { std::mem::zeroed::<SceCtrlData>() };
    unsafe {
        sceCtrlPeekBufferPositive(0, &mut ctrl, 1);
    }
    ctrl
}

fn read_front_touch() -> Option<(f32, f32)> {
    let mut touch = unsafe { std::mem::zeroed::<SceTouchData>() };
    let result = unsafe { sceTouchPeek(SCE_TOUCH_PORT_FRONT, &mut touch, 1) };
    if result <= 0 || touch.reportNum == 0 {
        return None;
    }

    let report = touch.report[0];
    let x = (report.x as f32 / VITA_TOUCH_W) * SCREEN_W as f32;
    let y = (report.y as f32 / VITA_TOUCH_H) * SCREEN_H as f32;
    Some((x.clamp(0.0, SCREEN_W as f32), y.clamp(0.0, SCREEN_H as f32)))
}

fn apply_touch_to_imgui(io: &mut imgui::Io, touch: Option<(f32, f32)>) {
    if let Some((x, y)) = touch {
        io.mouse_pos = [x, y];
        io.mouse_down[0] = true;
    } else {
        io.mouse_down[0] = false;
    }
}

fn apply_controller_to_imgui(io: &mut imgui::Io, ctrl: &SceCtrlData) {
    for key in io.keys_down.iter_mut() {
        *key = false;
    }
    set_key(io, VITA_IMGUI_KEY_UP, pressed(ctrl, SCE_CTRL_UP));
    set_key(io, VITA_IMGUI_KEY_DOWN, pressed(ctrl, SCE_CTRL_DOWN));
    set_key(io, VITA_IMGUI_KEY_LEFT, pressed(ctrl, SCE_CTRL_LEFT));
    set_key(io, VITA_IMGUI_KEY_RIGHT, pressed(ctrl, SCE_CTRL_RIGHT));
    set_key(io, VITA_IMGUI_KEY_CROSS, pressed(ctrl, SCE_CTRL_CROSS));
    set_key(io, VITA_IMGUI_KEY_CIRCLE, pressed(ctrl, SCE_CTRL_CIRCLE));

    for input in NavInput::VARIANTS {
        io.nav_inputs[input as usize] = 0.0;
    }

    set_nav_button(io, NavInput::Activate, pressed(ctrl, SCE_CTRL_CROSS));
    set_nav_button(io, NavInput::Cancel, pressed(ctrl, SCE_CTRL_CIRCLE));
    set_nav_button(io, NavInput::Menu, pressed(ctrl, SCE_CTRL_SQUARE));
    set_nav_button(io, NavInput::Input, pressed(ctrl, SCE_CTRL_TRIANGLE));
    set_nav_button(io, NavInput::DpadLeft, pressed(ctrl, SCE_CTRL_LEFT));
    set_nav_button(io, NavInput::DpadRight, pressed(ctrl, SCE_CTRL_RIGHT));
    set_nav_button(io, NavInput::DpadUp, pressed(ctrl, SCE_CTRL_UP));
    set_nav_button(io, NavInput::DpadDown, pressed(ctrl, SCE_CTRL_DOWN));
    set_nav_button(io, NavInput::FocusPrev, pressed(ctrl, SCE_CTRL_LTRIGGER));
    set_nav_button(io, NavInput::FocusNext, pressed(ctrl, SCE_CTRL_RTRIGGER));
}

fn pressed(ctrl: &SceCtrlData, button: u32) -> bool {
    (ctrl.buttons & button) != 0
}

fn set_key(io: &mut imgui::Io, key: u32, pressed: bool) {
    let idx = key as usize;
    if idx < io.keys_down.len() {
        io.keys_down[idx] = pressed;
    }
}

fn set_nav_button(io: &mut imgui::Io, input: NavInput, pressed: bool) {
    io.nav_inputs[input as usize] = if pressed { 1.0 } else { 0.0 };
}
