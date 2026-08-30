//! The layout, ported from `viewer.ts`.
//!
//! This is the part of the app that decides where every page is, which pages
//! are worth having in the document at all, and which page the reader is
//! looking at. In the app it is about six hundred lines of TypeScript spread
//! through a three-thousand-line file, entangled with the DOM it is placing
//! things in. Here it is a plain struct with no renderer, no widget and no
//! window in it, which is the first thing this port buys: `cargo test` can ask
//! it every question the harness had to open a browser to ask.
//!
//! Everything it knows is from `relayout()` in `viewer.ts`, and the comments
//! that explain *why* a line is the way it is are carried over with the line,
//! because those are the parts that were paid for.

/// A page's size, in PDF points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Size {
    pub width: f64,
    pub height: f64,
}

/// Where a page is, and how big: the layout's whole output.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PageBox {
    pub top: f64,
    pub left: f64,
    pub width: f64,
    pub height: f64,
    /// Points to pixels, for this page in this layout.
    pub scale: f64,
    /// The empty space directly above this page — the gap from the row before,
    /// or `PAD_Y` at the start of the document, and they are not the same
    /// number. Landing on a page means landing on that space, and it is
    /// recorded here rather than read back off the page before, which is right
    /// until two pages stand side by side and the box before this one is its
    /// neighbour, sharing its top exactly.
    pub above: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fit {
    Width,
    Page,
    Actual,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Spread {
    Single,
    Two,
    Cover,
}

/// What sets a page off from the window when it is narrower than one. Fit
/// width is the mode whose whole point is that it is not, so it does not get
/// one: charging it for the margin left forty pixels of ground either side of
/// a page that had supposedly reached both edges. `PAD_Y` is not conditional,
/// because there is always something above a page.
pub const PAD_X: f64 = 20.0;
pub const PAD_Y: f64 = 20.0;

/// How much beyond the viewport is kept in the document, as a fraction of a
/// screen either way. Under Blitz this matters more than it did under a
/// webview: every widget in the document is painted every frame whether or not
/// it is on screen, so a page that is not near the viewport must be genuinely
/// absent rather than merely invisible.
pub const OVERSCAN: f64 = 0.6;

/// The ceiling on one page's bitmap. A canvas had this to stay inside what a
/// browser would allocate; a texture has it because a page drawn at more
/// pixels than the screen can show is bytes nobody reads.
pub const MAX_PIXELS: f64 = 12_000_000.0;

/// pdf.js's own points-to-pixels, kept so that "100%" means here what it means
/// in the app.
pub const PDF_TO_CSS_UNITS: f64 = 96.0 / 72.0;

/// Where the reader is, described so that it survives a change of scale.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Anchor {
    /// One-based, as everything the reader sees is.
    pub page: usize,
    /// How far down that page, as a fraction of its height.
    pub offset: f64,
}

pub struct Layout {
    sizes: Vec<Size>,
    boxes: Vec<PageBox>,
    pub fit: Fit,
    /// The zoom factor, which only `Fit::Actual` reads.
    pub zoom: f64,
    pub spread: Spread,
    /// The distance between two rows, and between two pages of a spread.
    pub gap: f64,
    /// The scroller's own size, in CSS pixels.
    pub viewport: Size,
    content_width: f64,
    content_height: f64,
}

impl Layout {
    pub fn new(sizes: Vec<Size>) -> Self {
        let mut layout = Layout {
            sizes,
            boxes: Vec::new(),
            fit: Fit::Width,
            zoom: 1.0,
            spread: Spread::Single,
            gap: 16.0,
            viewport: Size {
                width: 900.0,
                height: 900.0,
            },
            content_width: 0.0,
            content_height: 0.0,
        };
        layout.relayout();
        layout
    }

    pub fn pages(&self) -> usize {
        self.sizes.len()
    }

    pub fn size_of(&self, index: usize) -> Size {
        self.sizes[index]
    }

    pub fn boxes(&self) -> &[PageBox] {
        &self.boxes
    }

    pub fn box_of(&self, index: usize) -> Option<&PageBox> {
        self.boxes.get(index)
    }

    pub fn content_width(&self) -> f64 {
        self.content_width
    }

    pub fn content_height(&self) -> f64 {
        self.content_height
    }

    /// The pages that stand together, in order.
    ///
    /// One each in single mode. In `Cover`, page one is alone and every pair
    /// after it is (even, odd) — which is how a book falls open, page one
    /// being a right-hand page. In `Two` the pairs start from the first page,
    /// which is what a document of slides or a scan of two-up photocopies
    /// wants.
    pub fn rows(&self) -> Vec<Vec<usize>> {
        let count = self.sizes.len();
        if self.spread == Spread::Single {
            return (0..count).map(|index| vec![index]).collect();
        }
        let mut rows = Vec::new();
        let mut index = 0;
        if self.spread == Spread::Cover && count > 0 {
            rows.push(vec![0]);
            index = 1;
        }
        while index < count {
            if index + 1 < count {
                rows.push(vec![index, index + 1]);
            } else {
                rows.push(vec![index]);
            }
            index += 2;
        }
        rows
    }

    /// The row a page is standing in, and the pages standing with it.
    pub fn row_of(&self, index: usize) -> Vec<usize> {
        let count = self.sizes.len();
        match self.spread {
            Spread::Single => vec![index],
            Spread::Cover => {
                if index == 0 {
                    return vec![0];
                }
                let first = if index % 2 == 1 { index } else { index - 1 };
                if first + 1 < count {
                    vec![first, first + 1]
                } else {
                    vec![first]
                }
            }
            Spread::Two => {
                let first = index - (index % 2);
                if first + 1 < count {
                    vec![first, first + 1]
                } else {
                    vec![first]
                }
            }
        }
    }

    /// Work out where every page is. The one place a page's position is
    /// decided, and the only thing that writes `boxes`.
    pub fn relayout(&mut self) {
        self.boxes.clear();
        if self.sizes.is_empty() {
            self.content_width = 0.0;
            self.content_height = 0.0;
            return;
        }

        let pad_x = if self.fit == Fit::Width { 0.0 } else { PAD_X };
        let available_width = (self.viewport.width - pad_x * 2.0).max(120.0);
        let available_height = (self.viewport.height - PAD_Y * 2.0).max(120.0);

        let rows = self.rows();

        // The gap between two pages of a spread is a distance on the screen,
        // like the gap between rows — it is not part of the page and does not
        // grow with the zoom. So it comes off the room available before the
        // scale is worked out, rather than being scaled along with the paper.
        let gaps_in = |row: &[usize]| (row.len() as f64 - 1.0) * self.gap;
        let paper_width =
            |row: &[usize]| row.iter().map(|&index| self.sizes[index].width).sum::<f64>();
        let row_height = |row: &[usize]| {
            row.iter()
                .map(|&index| self.sizes[index].height)
                .fold(0.0f64, f64::max)
        };
        let scale_for = |size: Size, room: f64| -> f64 {
            match self.fit {
                Fit::Width => room / size.width,
                Fit::Page => (room / size.width).min(available_height / size.height),
                Fit::Actual => PDF_TO_CSS_UNITS * self.zoom,
            }
        };
        let scale_for_row = |row: &[usize]| {
            scale_for(
                Size {
                    width: paper_width(row),
                    height: row_height(row),
                },
                (available_width - gaps_in(row)).max(120.0),
            )
        };
        let row_span = |row: &[usize], scale: f64| {
            row.iter()
                .map(|&index| (self.sizes[index].width * scale).round())
                .sum::<f64>()
                + gaps_in(row)
        };

        let mut width = 0.0f64;
        for row in &rows {
            width = width.max(row_span(row, scale_for_row(row)));
        }
        self.content_width = width.max(available_width) + pad_x * 2.0;

        let mut boxes = vec![
            PageBox {
                top: 0.0,
                left: 0.0,
                width: 0.0,
                height: 0.0,
                scale: 1.0,
                above: 0.0,
            };
            self.sizes.len()
        ];
        let mut top = PAD_Y;
        let mut above = PAD_Y;
        for row in &rows {
            let scale = scale_for_row(row);
            let across = row_span(row, scale);
            let mut left = ((self.content_width - across) / 2.0).round();
            let mut tallest = 0.0f64;
            for &index in row {
                let size = self.sizes[index];
                let page_width = (size.width * scale).round();
                let page_height = (size.height * scale).round();
                boxes[index] = PageBox {
                    top,
                    left,
                    width: page_width,
                    height: page_height,
                    scale,
                    above,
                };
                left += page_width + self.gap;
                tallest = tallest.max(page_height);
            }
            top += tallest + self.gap;
            above = self.gap;
        }
        self.boxes = boxes;
        self.content_height = (top - self.gap + PAD_Y).max(0.0);
    }

    /* Both searches below assume `boxes` runs in order down the page, which is
       true of every layout this struct produces. */

    /// The first page whose bottom edge is at or below `y`.
    pub fn first_box_ending_after(&self, y: f64) -> usize {
        let mut low = 0isize;
        let mut high = self.boxes.len() as isize - 1;
        let mut found = self.boxes.len();
        while low <= high {
            let middle = ((low + high) / 2) as usize;
            let page = &self.boxes[middle];
            if page.top + page.height >= y {
                found = middle;
                high = middle as isize - 1;
            } else {
                low = middle as isize + 1;
            }
        }
        found
    }

    /// The last page whose top edge is at or above `y`; page one if none is.
    pub fn last_box_starting_above(&self, y: f64) -> usize {
        let mut low = 0isize;
        let mut high = self.boxes.len() as isize - 1;
        let mut found = 0usize;
        while low <= high {
            let middle = ((low + high) / 2) as usize;
            if self.boxes[middle].top <= y {
                found = middle;
                low = middle as isize + 1;
            } else {
                high = middle as isize - 1;
            }
        }
        found
    }

    /// The pages that should be in the document at this scroll position.
    ///
    /// Boxes are in order down the page, so the visible run is found rather
    /// than looked for: scanning all of them cost a pass over the whole
    /// document on every frame of every scroll — nine hundred pages of work to
    /// discover that three of them are on screen.
    pub fn mounted(&self, scroll_top: f64) -> Vec<usize> {
        if self.boxes.is_empty() {
            return Vec::new();
        }
        let height = self.viewport.height;
        let from = scroll_top - height * OVERSCAN;
        let to = scroll_top + height * (1.0 + OVERSCAN);
        let mut wanted = Vec::new();
        let mut index = self.first_box_ending_after(from);
        while index < self.boxes.len() {
            if self.boxes[index].top > to {
                break;
            }
            wanted.push(index);
            index += 1;
        }
        wanted
    }

    /// Which page the reader is on: the row a third of the way down the
    /// window, and then the left-hand page of it — two pages standing side by
    /// side share a top, so the search finds the right-hand one, and a reader
    /// looking at a spread is on the page it opens at. One-based.
    pub fn page_at(&self, scroll_top: f64) -> usize {
        if self.boxes.is_empty() {
            return 1;
        }
        let probe = scroll_top + self.viewport.height * 0.35;
        self.row_of(self.last_box_starting_above(probe))[0] + 1
    }

    /// Where the reader is, in a form that survives a relayout.
    pub fn anchor(&self, scroll_top: f64) -> Anchor {
        if self.boxes.is_empty() {
            return Anchor {
                page: 1,
                offset: 0.0,
            };
        }
        let index = self.last_box_starting_above(scroll_top);
        let page = &self.boxes[index];
        Anchor {
            page: index + 1,
            offset: ((scroll_top - page.top) / page.height.max(1.0)).clamp(0.0, 1.0),
        }
    }

    /// Where the scroller has to be for a page to be at the top of the window.
    ///
    /// Landing on a page means the space above it starts at the top of the
    /// window and the page follows.
    pub fn scroll_target(&self, anchor: Anchor) -> f64 {
        let index = anchor.page.clamp(1, self.pages().max(1)) - 1;
        let Some(page) = self.boxes.get(index) else {
            return 0.0;
        };
        let target = page.top + anchor.offset * page.height
            - if anchor.offset == 0.0 { page.above } else { 0.0 };
        target.max(0.0).min(self.max_scroll())
    }

    pub fn max_scroll(&self) -> f64 {
        (self.content_height - self.viewport.height).max(0.0)
    }

    /// How many pixels a page is drawn at, which is its box in device pixels
    /// held under the ceiling. Returned as whole pixels, because a texture is.
    pub fn render_size(&self, index: usize, density: f64) -> (u32, u32) {
        let Some(page) = self.boxes.get(index) else {
            return (1, 1);
        };
        let mut width = page.width * density;
        let mut height = page.height * density;
        let pixels = width * height;
        if pixels > MAX_PIXELS {
            let shrink = (MAX_PIXELS / pixels).sqrt();
            width *= shrink;
            height *= shrink;
        }
        (
            (width.round() as u32).max(1),
            (height.round() as u32).max(1),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn letter(count: usize) -> Vec<Size> {
        (0..count)
            .map(|_| Size {
                width: 612.0,
                height: 792.0,
            })
            .collect()
    }

    fn reader(count: usize) -> Layout {
        let mut layout = Layout::new(letter(count));
        layout.viewport = Size {
            width: 900.0,
            height: 700.0,
        };
        layout.relayout();
        layout
    }

    #[test]
    fn fit_width_uses_the_whole_window() {
        let layout = reader(3);
        // No side margin in fit width: the content is exactly as wide as the
        // viewport, which is the fault the `padX` conditional exists to fix.
        assert_eq!(layout.content_width(), 900.0);
        assert_eq!(layout.box_of(0).unwrap().width, 900.0);
        assert_eq!(layout.box_of(0).unwrap().left, 0.0);
    }

    #[test]
    fn fit_page_keeps_the_margin_and_fits_the_height() {
        let mut layout = reader(3);
        layout.fit = Fit::Page;
        layout.relayout();
        let page = *layout.box_of(0).unwrap();
        assert!(page.height <= 700.0 - PAD_Y * 2.0 + 0.5, "{page:?}");
        assert!(page.left >= PAD_X, "a fitted page keeps its margin: {page:?}");
    }

    #[test]
    fn the_first_page_starts_below_the_top_margin() {
        let layout = reader(3);
        assert_eq!(layout.box_of(0).unwrap().top, PAD_Y);
        assert_eq!(layout.box_of(0).unwrap().above, PAD_Y);
        // And every page after it is a gap below the one before, which is the
        // other number `above` has to hold.
        assert_eq!(layout.box_of(1).unwrap().above, layout.gap);
    }

    #[test]
    fn landing_on_a_page_lands_on_the_space_above_it() {
        let layout = reader(5);
        for page in 1..=5 {
            let top = layout.scroll_target(Anchor {
                page,
                offset: 0.0,
            });
            let index = page - 1;
            let want = (layout.box_of(index).unwrap().top - layout.box_of(index).unwrap().above)
                .min(layout.max_scroll());
            assert!((top - want).abs() < 0.001, "page {page}: {top} vs {want}");
        }
    }

    #[test]
    fn the_searches_agree_with_a_scan() {
        let layout = reader(40);
        for y in (0..40_000).step_by(137).map(|y| y as f64) {
            let first = layout.first_box_ending_after(y);
            let scanned = layout
                .boxes()
                .iter()
                .position(|page| page.top + page.height >= y)
                .unwrap_or(layout.pages());
            assert_eq!(first, scanned, "first ending after {y}");

            let last = layout.last_box_starting_above(y);
            let scanned = layout
                .boxes()
                .iter()
                .rposition(|page| page.top <= y)
                .unwrap_or(0);
            assert_eq!(last, scanned, "last starting above {y}");
        }
    }

    #[test]
    fn only_the_pages_near_the_viewport_are_mounted() {
        let layout = reader(400);
        let mounted = layout.mounted(0.0);
        // A four-hundred page book is a handful of pages in the document, and
        // the number is bounded by the screen rather than by the book.
        assert!(mounted.len() < 8, "{} pages mounted", mounted.len());
        assert_eq!(mounted[0], 0);

        let deep = layout.scroll_target(Anchor {
            page: 200,
            offset: 0.0,
        });
        let mounted = layout.mounted(deep);
        assert!(mounted.contains(&199), "{mounted:?}");
        assert!(mounted.len() < 8, "{} pages mounted", mounted.len());

        // Every mounted page is within the overscan band, and no page outside
        // it is mounted: the two halves of the same claim.
        let from = deep - layout.viewport.height * OVERSCAN;
        let to = deep + layout.viewport.height * (1.0 + OVERSCAN);
        for (index, page) in layout.boxes().iter().enumerate() {
            let near = page.top + page.height >= from && page.top <= to;
            assert_eq!(near, mounted.contains(&index), "page {index}");
        }
    }

    #[test]
    fn the_page_being_read_is_the_one_a_third_down() {
        let layout = reader(10);
        assert_eq!(layout.page_at(0.0), 1);
        let at = layout.scroll_target(Anchor {
            page: 4,
            offset: 0.0,
        });
        assert_eq!(layout.page_at(at), 4);
    }

    #[test]
    fn an_anchor_survives_a_change_of_scale() {
        let mut layout = reader(20);
        let before = layout.anchor(layout.scroll_target(Anchor {
            page: 7,
            offset: 0.25,
        }));
        assert_eq!(before.page, 7);
        layout.fit = Fit::Actual;
        layout.zoom = 2.0;
        layout.relayout();
        let after = layout.anchor(layout.scroll_target(before));
        assert_eq!(after.page, 7);
        assert!((after.offset - before.offset).abs() < 0.01, "{after:?}");
    }

    #[test]
    fn a_cover_spread_opens_the_way_a_book_does() {
        let mut layout = reader(6);
        layout.spread = Spread::Cover;
        layout.relayout();
        assert_eq!(layout.rows(), vec![vec![0], vec![1, 2], vec![3, 4], vec![5]]);
        // Two pages side by side share a top exactly, which is the case
        // `above` and `page_at` are both careful about.
        assert_eq!(layout.box_of(1).unwrap().top, layout.box_of(2).unwrap().top);
        assert_eq!(layout.row_of(2), vec![1, 2]);
        // The gap between them is a distance on the screen: the pair spans
        // both pages and one gap, and no more.
        let left = layout.box_of(1).unwrap();
        let right = layout.box_of(2).unwrap();
        assert_eq!(right.left - (left.left + left.width), layout.gap);
    }

    #[test]
    fn a_page_is_never_drawn_at_more_than_the_ceiling() {
        let mut layout = Layout::new(vec![Size {
            width: 12_000.0,
            height: 16_000.0,
        }]);
        layout.viewport = Size {
            width: 1600.0,
            height: 1000.0,
        };
        layout.relayout();
        let (width, height) = layout.render_size(0, 2.0);
        let pixels = width as f64 * height as f64;
        assert!(pixels <= MAX_PIXELS, "{width}x{height} is {pixels} pixels");
        // And the shape is kept: a page held under the ceiling is the same
        // page, not a squashed one.
        let page = layout.box_of(0).unwrap();
        let ratio = width as f64 / height as f64;
        assert!((ratio - page.width / page.height).abs() < 0.01);
    }
}
