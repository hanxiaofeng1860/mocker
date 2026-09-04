use gpui::{px, size, Pixels, Size};

/// Design aspect used when the window is first opened (matches the original 1120×760).
pub const DESIGN_WIDTH: f32 = 1120.;
pub const DESIGN_HEIGHT: f32 = 760.;

pub fn fit_window_size(available: Size<Pixels>) -> Size<Pixels> {
    let avail_w = f32::from(available.width).max(1.);
    let avail_h = f32::from(available.height).max(1.);
    let aspect = DESIGN_WIDTH / DESIGN_HEIGHT;
    let mut height = avail_h;
    let mut width = height * aspect;
    if width > avail_w {
        width = avail_w;
        height = width / aspect;
    }
    size(px(width), px(height))
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::size;

    #[test]
    fn fit_window_uses_full_height_when_width_fits() {
        let size = fit_window_size(size(px(1440.), px(900.)));
        assert_eq!(f32::from(size.height), 900.);
        let width = f32::from(size.width);
        let expected = 900. * DESIGN_WIDTH / DESIGN_HEIGHT;
        assert!((width - expected).abs() < 0.5);
        assert!(width <= 1440.);
    }

    #[test]
    fn fit_window_shrinks_when_width_would_overflow() {
        let size = fit_window_size(size(px(800.), px(1200.)));
        assert_eq!(f32::from(size.width), 800.);
        let height = f32::from(size.height);
        let expected = 800. * DESIGN_HEIGHT / DESIGN_WIDTH;
        assert!((height - expected).abs() < 0.5);
        assert!(height <= 1200.);
    }
}
