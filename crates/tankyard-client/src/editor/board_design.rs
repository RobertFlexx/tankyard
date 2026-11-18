//! The board designer: a widget's grid of cells, laid out by clicking and
//! dragging in the inspector, with the picked cell's settings below it.
use super::inspector::Form;
use crate::ui::{
    accent, alpha, contains, fitted_label, label, label_width, line_color, text_color,
    ui_rectangle as draw_rectangle, ui_rectangle_lines as draw_rectangle_lines,
};
use crate::ui_layout;
use macroquad::prelude::*;
use std::cell::RefCell;
use tankyard_content::*;

/// What the designer remembers between frames.
#[derive(Default)]
struct Design {
    /// The cell being edited, by index.
    picked: Option<usize>,
    /// What clicking an empty square adds.
    kind: HudCellKind,
    drag: Option<Drag>,
    /// A message about the last thing that couldn't be done.
    notice: Option<&'static str>,
}

#[derive(Clone, Copy)]
enum Drag {
    /// Moving the picked cell, held this many squares from its corner.
    Move { grab: [i32; 2] },
    /// Pulling its bottom-right corner.
    Resize,
}

thread_local! {
    static DESIGN: RefCell<Design> = RefCell::new(Design::default());
}

/// Keep every cell inside a grid that may have shrunk.
pub(super) fn fit_cells(board: &mut HudBoard) {
    let side = [board.columns.max(1), board.rows.max(1)];
    for cell in &mut board.cells {
        for axis in 0..2 {
            cell.span[axis] = cell.span[axis].clamp(1, side[axis]);
            cell.at[axis] = cell.at[axis].min(side[axis] - cell.span[axis]);
        }
    }
}

/// The lowest channel nothing uses: neither the map, by `used`, nor this
/// board's buttons, nor any of `taken`.
fn free_after(board: &HudBoard, used: &[bool], taken: &[u8]) -> Option<u8> {
    (1..=MAP_CHANNELS).find(|&channel| {
        !used.get(usize::from(channel)).copied().unwrap_or(false)
            && !taken.contains(&channel)
            && !board
                .cells
                .iter()
                .any(|cell| cell.kind == HudCellKind::Button && cell.channel == channel)
    })
}
fn free_channel(board: &HudBoard, used: &[bool]) -> Option<u8> {
    free_after(board, used, &[])
}

/// The first place, row by row, where a block `span` squares big covers
/// no cell.
fn room_for(board: &HudBoard, span: [u8; 2]) -> Option<[u8; 2]> {
    if span[0] > board.columns || span[1] > board.rows {
        return None;
    }
    for row in 0..=board.rows - span[1] {
        for column in 0..=board.columns - span[0] {
            let clear = (row..row + span[1])
                .all(|r| (column..column + span[0]).all(|c| board.cell_at(c, r).is_none()));
            if clear {
                return Some([column, row]);
            }
        }
    }
    None
}

/// A new cell of `kind` at `at`, ready to read.
fn new_cell(kind: HudCellKind, at: [u8; 2], channel: Option<u8>) -> HudCell {
    let mut cell = HudCell {
        kind,
        at,
        ..HudCell::default()
    };
    match kind {
        HudCellKind::Label => cell.text = "Label".into(),
        HudCellKind::Button => {
            cell.channel = channel.unwrap_or(0);
            cell.text = cell.channel.to_string();
        }
        HudCellKind::Screen => cell.text = "Ready".into(),
        HudCellKind::Digits => cell.text = "0".into(),
        HudCellKind::Light => {}
        HudCellKind::Meter => cell.counter = 1,
    }
    cell
}

/// How a cell of `kind` shows in the designer.
fn swatch(kind: HudCellKind) -> Color {
    match kind {
        HudCellKind::Label => Color::new(0.55, 0.58, 0.64, 1.),
        HudCellKind::Button => accent(),
        HudCellKind::Screen => Color::new(0.2, 0.75, 0.55, 1.),
        HudCellKind::Digits => Color::new(1., 0.62, 0.2, 1.),
        HudCellKind::Light => Color::new(0.95, 0.85, 0.3, 1.),
        HudCellKind::Meter => Color::new(0.5, 0.45, 0.95, 1.),
    }
}

/// Edit `board`: its size, the grid of cells, then the picked cell. `used`
/// marks the channels the map already signals on, by number.
pub(super) fn board_form(f: &mut Form, board: &mut HudBoard, used: &[bool]) {
    DESIGN.with(|design| edit(f, board, used, &mut design.borrow_mut()));
}

fn edit(f: &mut Form, board: &mut HudBoard, used: &[bool], design: &mut Design) {
    let s = f.s;
    if design
        .picked
        .is_some_and(|index| index >= board.cells.len())
    {
        design.picked = None;
        design.drag = None;
    }
    f.stepper("Columns", &mut board.columns, [1, HUD_BOARD_SIDE], None);
    f.stepper("Rows", &mut board.rows, [1, HUD_BOARD_SIDE], None);
    fit_cells(board);
    f.number("Square size", &mut board.cell, HudBoard::CELL_RANGE, 1.);
    f.number("Gap", &mut board.gap, HudBoard::GAP_RANGE, 1.);
    f.pick(
        "Click adds",
        &mut design.kind,
        &HudCellKind::ALL,
        HudCellKind::name,
        3,
    );

    // The grid: squares as wide as the column allows, up to a comfortable
    // size, with each cell drawn over the squares it covers.
    let (columns, rows) = (f32::from(board.columns), f32::from(board.rows));
    let unit = (f.w / columns).min(34. * s);
    let grid = Rect::new(f.x, f.y + 4. * s, unit * columns, unit * rows);
    draw_rectangle(grid.x, grid.y, grid.w, grid.h, alpha(text_color(), 0.05));
    for column in 1..board.columns {
        let x = grid.x + f32::from(column) * unit;
        draw_rectangle(x, grid.y, 1., grid.h, alpha(line_color(), 0.5));
    }
    for row in 1..board.rows {
        let y = grid.y + f32::from(row) * unit;
        draw_rectangle(grid.x, y, grid.w, 1., alpha(line_color(), 0.5));
    }
    let place = |cell: &HudCell| {
        Rect::new(
            grid.x + f32::from(cell.at[0]) * unit + 2.,
            grid.y + f32::from(cell.at[1]) * unit + 2.,
            f32::from(cell.span[0]) * unit - 4.,
            f32::from(cell.span[1]) * unit - 4.,
        )
    };
    let handle = |rect: Rect| {
        let size = (9. * s).min(rect.w * 0.5).min(rect.h * 0.5);
        Rect::new(rect.right() - size, rect.bottom() - size, size, size)
    };
    for (index, cell) in board.cells.iter().enumerate() {
        let rect = place(cell);
        let tint = swatch(cell.kind);
        let picked = design.picked == Some(index);
        draw_rectangle(
            rect.x,
            rect.y,
            rect.w,
            rect.h,
            alpha(tint, if picked { 0.45 } else { 0.25 }),
        );
        draw_rectangle_lines(
            rect.x,
            rect.y,
            rect.w,
            rect.h,
            if picked { 2.5 * s } else { 1. },
            if picked { accent() } else { alpha(tint, 0.9) },
        );
        let words = match (cell.kind, cell.text.is_empty()) {
            (HudCellKind::Light, true) => "\u{25cf}".to_owned(),
            (HudCellKind::Meter, true) => format!("#{}", cell.counter),
            _ => cell.text.clone(),
        };
        let size = (unit * 0.38).clamp(8. * s, 13. * s);
        let shown = fitted_label(&words, size, rect.w - 4.);
        let width = label_width(&shown, size);
        label(
            &shown,
            rect.x + (rect.w - width) * 0.5,
            rect.y + rect.h * 0.5 + size * 0.35,
            size,
            text_color(),
        );
        if picked {
            let corner = handle(rect);
            draw_rectangle(corner.x, corner.y, corner.w, corner.h, accent());
        }
    }
    draw_rectangle_lines(grid.x, grid.y, grid.w, grid.h, 1.5, line_color());

    // Clicks: the picked cell's corner resizes it, a cell is picked and
    // dragged, and an empty square gets a new cell.
    let pointer = ui_layout::pointer();
    let square = |p: Vec2| {
        [
            ((p.x - grid.x) / unit).floor().clamp(0., columns - 1.) as i32,
            ((p.y - grid.y) / unit).floor().clamp(0., rows - 1.) as i32,
        ]
    };
    if contains(grid) && is_mouse_button_pressed(MouseButton::Left) {
        let [column, row] = square(pointer);
        let on_handle = design
            .picked
            .and_then(|index| board.cells.get(index))
            .is_some_and(|cell| handle(place(cell)).contains(pointer));
        design.notice = None;
        if on_handle {
            design.drag = Some(Drag::Resize);
        } else if let Some(index) = board.cell_at(column as u8, row as u8) {
            let at = board.cells[index].at;
            design.picked = Some(index);
            design.drag = Some(Drag::Move {
                grab: [column - i32::from(at[0]), row - i32::from(at[1])],
            });
        } else if board.cells.len() < HUD_CELLS {
            let channel = free_channel(board, used);
            board
                .cells
                .push(new_cell(design.kind, [column as u8, row as u8], channel));
            design.picked = Some(board.cells.len() - 1);
            design.drag = None;
        } else {
            design.notice = Some("A board holds 128 cells at most.");
        }
    }
    if is_mouse_button_down(MouseButton::Left) {
        if let (Some(drag), Some(cell)) = (
            design.drag,
            design.picked.and_then(|i| board.cells.get_mut(i)),
        ) {
            let [column, row] = square(pointer);
            let side = [i32::from(board.columns), i32::from(board.rows)];
            match drag {
                Drag::Move { grab } => {
                    for (axis, spot) in [column, row].into_iter().enumerate() {
                        let span = i32::from(cell.span[axis]);
                        cell.at[axis] = (spot - grab[axis]).clamp(0, side[axis] - span) as u8;
                    }
                }
                Drag::Resize => {
                    for (axis, spot) in [column, row].into_iter().enumerate() {
                        let at = i32::from(cell.at[axis]);
                        cell.span[axis] = (spot - at + 1).clamp(1, side[axis] - at) as u8;
                    }
                }
            }
        }
    } else {
        design.drag = None;
    }
    f.y = grid.bottom() + 6. * s;
    f.note(&format!(
        "{} of {} cells. Click a square to add one, drag a cell to move it, or drag its corner to resize it.",
        board.cells.len(),
        HUD_CELLS
    ));
    if let Some(notice) = design.notice {
        f.note(notice);
    }

    match f.buttons(&["Copy cell", "Remove cell", "Clear all"], true) {
        Some(0) => match design.picked.and_then(|index| board.cells.get(index)) {
            Some(cell) if board.cells.len() < HUD_CELLS => match room_for(board, cell.span) {
                Some(at) => {
                    let mut copy = HudCell { at, ..cell.clone() };
                    // Numbered keys count on.
                    if let Ok(number) = copy.text.trim().parse::<i64>() {
                        copy.text = (number + 1).to_string();
                    }
                    if copy.kind == HudCellKind::Button {
                        copy.channel = free_channel(board, used).unwrap_or(0);
                    }
                    board.cells.push(copy);
                    design.picked = Some(board.cells.len() - 1);
                }
                None => design.notice = Some("No room for a copy: add columns or rows."),
            },
            Some(_) => design.notice = Some("A board holds 128 cells at most."),
            None => design.notice = Some("Pick a cell to copy first."),
        },
        Some(1) => {
            if let Some(index) = design.picked.take() {
                board.cells.remove(index);
            }
        }
        Some(2) => {
            board.cells.clear();
            design.picked = None;
        }
        _ => {}
    }
    match f.buttons(&["Add number keys", "Add 8 bit lights"], true) {
        Some(0) => add_keypad(board, used, design),
        Some(1) => add_bit_lights(board, design),
        _ => {}
    }

    let Some(index) = design.picked else {
        f.note("Buttons signal their channel when clicked, with the player who clicked as the tank that set it off. Lights and buttons light up from a counter, one bit of it or a group. Screens and digits show text with counter fields, filled in as counters change.");
        return;
    };
    let previous = f.set_salt(0xb0a2_d000 + index as u64);
    let free = free_channel(board, used);
    let (columns, rows) = (board.columns, board.rows);
    let cell = &mut board.cells[index];
    f.heading(&format!("Cell {} ({})", index + 1, cell.kind.name()));
    f.pick(
        "Kind",
        &mut cell.kind,
        &HudCellKind::ALL,
        HudCellKind::name,
        3,
    );
    f.text("Text", &mut cell.text, HUD_CELL_TEXT);
    f.stepper("Column", &mut cell.at[0], [0, columns - cell.span[0]], None);
    f.stepper("Row", &mut cell.at[1], [0, rows - cell.span[1]], None);
    f.stepper("Width", &mut cell.span[0], [1, columns - cell.at[0]], None);
    f.stepper("Height", &mut cell.span[1], [1, rows - cell.at[1]], None);
    match cell.kind {
        HudCellKind::Button => {
            f.stepper(
                "Signals channel",
                &mut cell.channel,
                [0, MAP_CHANNELS],
                Some(free),
            );
            lit_form(f, cell);
        }
        HudCellKind::Light => lit_form(f, cell),
        HudCellKind::Meter => {
            f.stepper("Counter", &mut cell.counter, [0, MAP_COUNTERS], None);
            cell.max = cell.max.clamp(1, COUNTER_LIMIT);
            f.integer("Full at", &mut cell.max, [1, COUNTER_LIMIT]);
            cell.bit = 0;
            cell.group = 0;
        }
        _ => {
            cell.counter = 0;
            cell.bit = 0;
            cell.group = 0;
        }
    }
    if cell.kind != HudCellKind::Button {
        cell.channel = 0;
    }
    let mut size = cell.size;
    if f.number(
        "Text size (0 fits the cell)",
        &mut size,
        [0., HudCell::SIZE_RANGE[1]],
        0.05,
    ) {
        // Between none and the smallest, the smallest.
        cell.size = if size <= 0. {
            0.
        } else {
            size.max(HudCell::SIZE_RANGE[0])
        };
    }
    f.pick(
        "Line up",
        &mut cell.align,
        &HudAlign::ALL,
        HudAlign::name,
        3,
    );
    let defaults = [[255, 196, 64], [24, 30, 40]];
    for ((title, color), fallback) in ["Own color", "Own background"]
        .into_iter()
        .zip([&mut cell.color, &mut cell.fill])
        .zip(defaults)
    {
        let mut own = color.is_some();
        if f.toggle(title, &mut own) {
            *color = own.then_some(color.unwrap_or(fallback));
        }
        if let Some(rgb) = color {
            f.color(rgb);
        }
    }
    f.note("{3} shows counter 3. {3:04} pads it to 4 digits, {3:,} groups thousands, {3:b} and {3:x} show binary and hex, and {3|2|1:4} joins counters 3, 2 and 1 as 4-digit parts of one big number.");
    f.set_salt(previous);
}

/// What lights a button or light: nothing, a counter that isn't 0, one
/// bit of a counter, or a group while it shows.
fn lit_form(f: &mut Form, cell: &mut HudCell) {
    #[derive(Clone, Copy, PartialEq)]
    enum Lit {
        Never,
        Counter,
        Bit,
        Group,
    }
    let mut lit = match (cell.group, cell.counter, cell.bit) {
        (group, _, _) if group > 0 => Lit::Group,
        (_, 0, _) => Lit::Never,
        (_, _, 0) => Lit::Counter,
        _ => Lit::Bit,
    };
    let before = lit;
    f.cycle(
        "Lit by",
        &mut lit,
        &[Lit::Never, Lit::Counter, Lit::Bit, Lit::Group],
        |lit| {
            match lit {
                Lit::Never => "Nothing",
                Lit::Counter => "A counter not at 0",
                Lit::Bit => "One bit of a counter",
                Lit::Group => "A group while shown",
            }
            .to_owned()
        },
    );
    if lit != before {
        cell.counter = u8::from(matches!(lit, Lit::Counter | Lit::Bit));
        cell.bit = u8::from(lit == Lit::Bit);
        cell.group = u8::from(lit == Lit::Group);
        cell.lit = false;
    }
    match lit {
        Lit::Never => {}
        Lit::Counter | Lit::Bit => {
            cell.counter = cell.counter.clamp(1, MAP_COUNTERS);
            f.stepper("Counter", &mut cell.counter, [1, MAP_COUNTERS], None);
            if lit == Lit::Bit {
                cell.bit = cell.bit.clamp(1, HudCell::BITS);
                f.stepper(
                    "Bit (1 is the lowest)",
                    &mut cell.bit,
                    [1, HudCell::BITS],
                    None,
                );
            }
        }
        Lit::Group => {
            cell.group = cell.group.clamp(1, MAP_GROUPS);
            f.stepper("Group", &mut cell.group, [1, MAP_GROUPS], None);
        }
    }
    if lit == Lit::Never {
        f.toggle("Always lit", &mut cell.lit);
    }
}

/// Keys 0 to 9 in the usual three columns, each on a free channel, in
/// order from 0.
fn add_keypad(board: &mut HudBoard, used: &[bool], design: &mut Design) {
    let Some([x, y]) = room_for(board, [3, 4]) else {
        design.notice = Some("Number keys need a clear block 3 squares wide and 4 high.");
        return;
    };
    if board.cells.len() + 10 > HUD_CELLS {
        design.notice = Some("A board holds 128 cells at most.");
        return;
    }
    let mut channels = Vec::with_capacity(10);
    for _ in 0..10 {
        let Some(channel) = free_after(board, used, &channels) else {
            design.notice = Some("Not enough free signal channels for ten keys.");
            return;
        };
        channels.push(channel);
    }
    let keys: [(u8, [u8; 2], u8); 10] = [
        (7, [0, 0], 1),
        (8, [1, 0], 1),
        (9, [2, 0], 1),
        (4, [0, 1], 1),
        (5, [1, 1], 1),
        (6, [2, 1], 1),
        (1, [0, 2], 1),
        (2, [1, 2], 1),
        (3, [2, 2], 1),
        (0, [0, 3], 3),
    ];
    for (digit, [column, row], width) in keys {
        board.cells.push(HudCell {
            kind: HudCellKind::Button,
            at: [x + column, y + row],
            span: [width, 1],
            text: digit.to_string(),
            channel: channels[usize::from(digit)],
            ..HudCell::default()
        });
    }
    design.picked = None;
    design.notice = Some(
        "Added keys 0 to 9: key 0 signals the first new channel, and each key after it the next.",
    );
}

/// Eight lights in a row showing bits 8 down to 1 of counter 1, the way
/// binary is written.
fn add_bit_lights(board: &mut HudBoard, design: &mut Design) {
    let Some([x, y]) = room_for(board, [8, 1]) else {
        design.notice = Some("Bit lights need a clear row 8 squares wide.");
        return;
    };
    if board.cells.len() + 8 > HUD_CELLS {
        design.notice = Some("A board holds 128 cells at most.");
        return;
    }
    for place in 0..8u8 {
        board.cells.push(HudCell {
            kind: HudCellKind::Light,
            at: [x + place, y],
            counter: 1,
            bit: 8 - place,
            ..HudCell::default()
        });
    }
    design.picked = None;
    design.notice =
        Some("Added lights for bits 8 to 1 of counter 1. Pick one to change its counter.");
}

/// A sample board's cells with sample values, as the inspector's picture
/// shows them: `fill` fills in text fields.
pub(super) fn sample(board: &mut HudBoard, counters: &[i32], fill: impl Fn(&str) -> String) {
    for cell in &mut board.cells {
        cell.text = clip_text(&fill(&cell.text), HUD_CELL_TEXT);
        if cell.watches() {
            cell.lit = cell.lit_by(counters, 0);
        }
        if cell.kind == HudCellKind::Meter && cell.counter > 0 {
            cell.progress = cell.fill_with(counters);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn board(cells: Vec<HudCell>) -> HudBoard {
        HudBoard {
            columns: 4,
            rows: 4,
            cells,
            ..HudBoard::default()
        }
    }

    #[test]
    fn shrinking_the_grid_keeps_cells_inside_and_valid() {
        let mut b = board(vec![
            HudCell {
                at: [3, 3],
                span: [1, 1],
                ..HudCell::default()
            },
            HudCell {
                at: [0, 0],
                span: [4, 2],
                ..HudCell::default()
            },
        ]);
        b.columns = 2;
        b.rows = 1;
        fit_cells(&mut b);
        assert!(b.valid(), "{b:?}");
        assert_eq!(b.cells[0].at, [1, 0]);
        assert_eq!(b.cells[1].span, [2, 1]);
    }

    #[test]
    fn keypads_and_copies_find_room_and_free_channels() {
        let mut used = vec![false; usize::from(MAP_CHANNELS) + 1];
        used[1] = true;
        used[3] = true;
        let mut b = board(vec![HudCell {
            kind: HudCellKind::Button,
            channel: 2,
            ..HudCell::default()
        }]);
        assert_eq!(free_channel(&b, &used), Some(4));
        let mut design = Design::default();
        add_keypad(&mut b, &used, &mut design);
        assert_eq!(b.cells.len(), 11);
        assert!(b.valid());
        let channel = |digit: &str| b.cells.iter().find(|c| c.text == digit).unwrap().channel;
        assert_eq!(channel("0"), 4);
        assert_eq!(channel("9"), 13);
        // The keypad sits clear of the first cell.
        assert_eq!(b.cells[1].at, [1, 0]);
        assert_eq!(room_for(&b, [1, 1]), Some([0, 1]));
        add_keypad(&mut b, &used, &mut design);
        assert_eq!(b.cells.len(), 11, "no room for a second");
        assert!(design.notice.is_some());
    }
}
