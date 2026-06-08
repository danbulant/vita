use imgui::Ui;

const GRID_COLUMNS: usize = 3;
const GRID_GAP: f32 = 14.0;
const GRID_CELL_HEIGHT: f32 = 118.0;
const ICON_SIZE: f32 = 42.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LibraryHomeAction {
    Artists,
    Albums,
    Tracks,
    Roots,
    BrowseFiles,
    ScanCurrentFolder,
}

#[derive(Clone, Copy)]
enum LibraryHomeIcon {
    Artists,
    Albums,
    Tracks,
    Roots,
    BrowseFiles,
    Scan,
}

struct LibraryHomeItem<'a> {
    label: &'a str,
    id: &'a str,
    icon: LibraryHomeIcon,
    action: LibraryHomeAction,
}

pub fn draw_library_home(
    ui: &Ui,
    disable_hover: bool,
    current_folder_action_label: Option<&str>,
) -> Option<LibraryHomeAction> {
    let mut items = vec![
        LibraryHomeItem {
            label: "Artists",
            id: "artists",
            icon: LibraryHomeIcon::Artists,
            action: LibraryHomeAction::Artists,
        },
        LibraryHomeItem {
            label: "Albums",
            id: "albums",
            icon: LibraryHomeIcon::Albums,
            action: LibraryHomeAction::Albums,
        },
        LibraryHomeItem {
            label: "Tracks",
            id: "tracks",
            icon: LibraryHomeIcon::Tracks,
            action: LibraryHomeAction::Tracks,
        },
        LibraryHomeItem {
            label: "Roots",
            id: "roots",
            icon: LibraryHomeIcon::Roots,
            action: LibraryHomeAction::Roots,
        },
        LibraryHomeItem {
            label: "Browse files",
            id: "browse-files",
            icon: LibraryHomeIcon::BrowseFiles,
            action: LibraryHomeAction::BrowseFiles,
        },
    ];

    if let Some(label) = current_folder_action_label {
        items.push(LibraryHomeItem {
            label,
            id: "scan-current-folder",
            icon: LibraryHomeIcon::Scan,
            action: LibraryHomeAction::ScanCurrentFolder,
        });
    }

    let start = ui.cursor_pos();
    let avail_w = ui.content_region_avail()[0].max(1.0);
    let cell_w = ((avail_w - GRID_GAP * (GRID_COLUMNS - 1) as f32) / GRID_COLUMNS as f32).max(96.0);
    let mut action = None;

    for (idx, item) in items.iter().enumerate() {
        let col = idx % GRID_COLUMNS;
        let row = idx / GRID_COLUMNS;
        ui.set_cursor_pos([
            start[0] + col as f32 * (cell_w + GRID_GAP),
            start[1] + row as f32 * (GRID_CELL_HEIGHT + GRID_GAP),
        ]);

        if icon_grid_button(ui, item, [cell_w, GRID_CELL_HEIGHT], disable_hover) {
            action = Some(item.action);
        }
    }

    let rows = (items.len() + GRID_COLUMNS - 1) / GRID_COLUMNS;
    ui.set_cursor_pos([
        start[0],
        start[1] + rows as f32 * (GRID_CELL_HEIGHT + GRID_GAP),
    ]);

    action
}

fn icon_grid_button(
    ui: &Ui,
    item: &LibraryHomeItem<'_>,
    size: [f32; 2],
    disable_hover: bool,
) -> bool {
    let clicked = ui
        .selectable_config(&format!("##library-home-{}", item.id))
        .disabled(disable_hover)
        .size(size)
        .build();

    let min = ui.item_rect_min();
    let max = ui.item_rect_max();
    let hovered = !disable_hover && ui.is_item_hovered();
    let bg = if hovered {
        [0.20, 0.25, 0.32, 1.0]
    } else {
        [0.13, 0.16, 0.21, 1.0]
    };
    let border = if hovered {
        [0.42, 0.72, 1.0, 1.0]
    } else {
        [0.28, 0.34, 0.42, 1.0]
    };

    {
        let draw_list = ui.get_window_draw_list();
        draw_list
            .add_rect(min, max, bg)
            .rounding(8.0)
            .filled(true)
            .build();
        draw_list
            .add_rect(min, max, border)
            .rounding(8.0)
            .thickness(2.0)
            .build();
    }

    let center_x = (min[0] + max[0]) * 0.5;
    let icon_pos = [center_x - ICON_SIZE * 0.5, min[1] + 22.0];
    draw_library_home_icon(ui, item.icon, icon_pos, ICON_SIZE);

    let text_size = ui.calc_text_size(item.label);
    let text_x = center_x - text_size[0] * 0.5;
    let text_y = icon_pos[1] + ICON_SIZE + 14.0;
    ui.get_window_draw_list()
        .add_text([text_x, text_y], [0.92, 0.94, 0.98, 1.0], item.label);

    clicked
}

fn draw_library_home_icon(ui: &Ui, icon: LibraryHomeIcon, pos: [f32; 2], size: f32) {
    let draw_list = ui.get_window_draw_list();
    let color = [0.86, 0.90, 0.96, 1.0];
    let accent = [0.42, 0.72, 1.0, 1.0];
    let x = pos[0];
    let y = pos[1];
    let s = size;

    match icon {
        LibraryHomeIcon::Artists => {
            draw_list
                .add_circle([x + s * 0.34, y + s * 0.34], s * 0.15, accent)
                .filled(true)
                .build();
            draw_list
                .add_circle([x + s * 0.66, y + s * 0.34], s * 0.15, color)
                .filled(true)
                .build();
            draw_list
                .add_circle([x + s * 0.50, y + s * 0.62], s * 0.17, color)
                .filled(true)
                .build();
        }
        LibraryHomeIcon::Albums => {
            draw_list
                .add_rect(
                    [x + s * 0.18, y + s * 0.10],
                    [x + s * 0.82, y + s * 0.90],
                    color,
                )
                .rounding(3.0)
                .filled(true)
                .build();
            draw_list
                .add_circle([x + s * 0.50, y + s * 0.50], s * 0.22, accent)
                .filled(true)
                .build();
            draw_list
                .add_circle([x + s * 0.50, y + s * 0.50], s * 0.07, color)
                .filled(true)
                .build();
        }
        LibraryHomeIcon::Tracks => {
            draw_list
                .add_line(
                    [x + s * 0.58, y + s * 0.16],
                    [x + s * 0.58, y + s * 0.72],
                    accent,
                )
                .thickness(4.0)
                .build();
            draw_list
                .add_line(
                    [x + s * 0.58, y + s * 0.16],
                    [x + s * 0.84, y + s * 0.26],
                    accent,
                )
                .thickness(4.0)
                .build();
            draw_list
                .add_circle([x + s * 0.40, y + s * 0.76], s * 0.18, color)
                .filled(true)
                .build();
        }
        LibraryHomeIcon::Roots | LibraryHomeIcon::BrowseFiles => {
            draw_list
                .add_rect(
                    [x + s * 0.08, y + s * 0.28],
                    [x + s * 0.44, y + s * 0.46],
                    accent,
                )
                .filled(true)
                .build();
            draw_list
                .add_rect(
                    [x + s * 0.08, y + s * 0.40],
                    [x + s * 0.92, y + s * 0.84],
                    color,
                )
                .rounding(3.0)
                .filled(true)
                .build();
            if matches!(icon, LibraryHomeIcon::Roots) {
                draw_list
                    .add_line(
                        [x + s * 0.26, y + s * 0.62],
                        [x + s * 0.74, y + s * 0.62],
                        accent,
                    )
                    .thickness(3.0)
                    .build();
            }
        }
        LibraryHomeIcon::Scan => {
            draw_list
                .add_circle([x + s * 0.44, y + s * 0.42], s * 0.24, color)
                .thickness(4.0)
                .build();
            draw_list
                .add_line(
                    [x + s * 0.62, y + s * 0.62],
                    [x + s * 0.86, y + s * 0.86],
                    accent,
                )
                .thickness(5.0)
                .build();
        }
    }
}
