//! The renderer, with one thing put right on the way through.
//!
//! **A frame that fails to reach the screen is not thrown away, and the next
//! one is drawn on top of it.** `render` in `anyrender_vello_hybrid` builds
//! the scene, asks the surface for a texture to draw into, and only resets the
//! scene at the very end — after the present has succeeded. There are two
//! early returns before that point, one for a surface texture that cannot be
//! had and one for a present that fails, and neither of them resets. So a
//! window whose surface has gone away keeps accumulating: every redraw appends
//! another whole frame to the same scene, the alpha buffer behind it grows
//! without bound, and eventually `vello_common` asserts —
//!
//! ```text
//! thread 'main' panicked at vello_common-0.1.0/src/strip.rs:205:
//! `alpha_idx` too large
//! ```
//!
//! — which is a `u32` running into the bit that `Strip` packs its fill flag
//! into, at about two billion. It takes a while to get there, which is why the
//! way to see it is to leave the reader open and let the machine's display go
//! to sleep. The app dies while nobody is looking at it and the next thing the
//! reader sees is that it is gone.
//!
//! Blitz already declines to paint an *occluded* window, which is the case
//! macOS reports; this is the case it does not, and it is not specific to
//! sleep — a `Timeout` from `get_current_texture` during any surface hiccup
//! poisons the scene in exactly the same way and never recovers.
//!
//! The fix upstream is to reset on those two paths. From outside the crate
//! there is still one hook: the draw closure is handed the scene, and
//! `PaintScene` has `reset` on it. So this wrapper resets at the *start* of
//! every frame, which discards whatever a failed frame left behind and costs
//! nothing when there was none. The base colour is filled back in because the
//! reset takes the inner renderer's own fill with it — that fill is the first
//! thing in the scene and `Color::WHITE` is what it uses when nothing has
//! asked for another.
//!
//! Everything else here is delegation, and it is only written out because
//! `WindowRenderer` is a trait with eight methods rather than something with a
//! blanket impl.

use std::any::Any;
use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

use anyrender::{
    PaintScene, RegisterResourceError, RenderContext, ResourceId, WindowHandle, WindowRenderer,
};
use dioxus_native::DioxusNativeWindowRenderer;
use peniko::kurbo::{Affine, Rect};
use peniko::{Color, Fill};

/// The renderer every window in this app is given. See the module comment.
#[derive(Clone)]
pub struct Steady {
    inner: DioxusNativeWindowRenderer,
    /// What to fill the ground with, in the size the surface was last told
    /// about. `set_size` is the only thing that knows it and `render` is the
    /// only thing that wants it.
    size: Rc<Cell<(u32, u32)>>,
}

impl Steady {
    pub fn new() -> Self {
        Steady {
            inner: DioxusNativeWindowRenderer::new(),
            size: Rc::new(Cell::new((0, 0))),
        }
    }
}

impl Default for Steady {
    fn default() -> Self {
        Self::new()
    }
}

impl RenderContext for Steady {
    fn try_register_custom_resource(
        &mut self,
        resource: Box<dyn Any>,
    ) -> Result<ResourceId, RegisterResourceError> {
        self.inner.try_register_custom_resource(resource)
    }

    fn unregister_resource(&mut self, resource_id: ResourceId) {
        self.inner.unregister_resource(resource_id);
    }

    fn renderer_specific_context(&self) -> Option<Box<dyn Any>> {
        self.inner.renderer_specific_context()
    }
}

impl WindowRenderer for Steady {
    type ScenePainter<'a>
        = <DioxusNativeWindowRenderer as WindowRenderer>::ScenePainter<'a>
    where
        Self: 'a;

    fn resume<F: FnOnce() + 'static>(
        &mut self,
        window: Arc<dyn WindowHandle>,
        width: u32,
        height: u32,
        on_ready: F,
    ) {
        self.size.set((width, height));
        self.inner.resume(window, width, height, on_ready);
    }

    fn complete_resume(&mut self) -> bool {
        self.inner.complete_resume()
    }

    fn suspend(&mut self) {
        self.inner.suspend();
    }

    fn is_active(&self) -> bool {
        self.inner.is_active()
    }

    fn is_pending(&self) -> bool {
        self.inner.is_pending()
    }

    fn set_size(&mut self, width: u32, height: u32) {
        self.size.set((width, height));
        self.inner.set_size(width, height);
    }

    fn render<F: FnOnce(&mut Self::ScenePainter<'_>)>(&mut self, draw_fn: F) {
        let (width, height) = self.size.get();
        self.inner.render(|scene| {
            // Whatever a failed frame left behind, and the inner renderer's
            // own base fill with it.
            scene.reset();
            scene.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                Color::WHITE,
                None,
                &Rect::new(0.0, 0.0, width as f64, height as f64),
            );
            draw_fn(scene);
        });
    }
}
