//! # UI – Zed UI Primitives & Components
//!
//! This crate provides a set of UI primitives and components that are used to build all of the elements in Zed's UI.
//!
//! ## Related Crates:
//!
//! - [`ui_macros`] - proc_macros support for this crate
//! - `ui_input` - the single line input component

mod arcoscope;
mod arcoscope_skin;
pub mod component_prelude;
mod components;
pub mod prelude;
mod styles;
mod traits;
pub mod utils;

pub use arcoscope::*;
pub use arcoscope_skin::{
    ArcoscopeSkinSeam, arcoscope_skin_content_corners, arcoscope_skin_padding,
    arcoscope_skin_padding_alone, arcoscope_skin_surface, arcoscope_skin_surface_alone,
    arcoscope_skin_surface_variant, arcoscope_skin_surface_variant_rounded, has_arcoscope_skin,
    paint_arcoscope_skin,
};
pub use components::*;
pub use prelude::*;
pub use styles::*;
pub use traits::animation_ext::*;
