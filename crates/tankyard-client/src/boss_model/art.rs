//! Painted reliefs set into the moving geometry. The renderer uses a baked
//! copy of each drawing; the model sheet places the original scalable art.
use super::*;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModelArt {
    ComedyMask,
    CastBell,
    JingleBell,
}

impl ModelArt {
    pub const ALL: [Self; 3] = [Self::ComedyMask, Self::CastBell, Self::JingleBell];
    pub fn size(self) -> Vec2 {
        match self {
            Self::ComedyMask => vec2(160., 208.),
            Self::CastBell => vec2(200., 200.),
            Self::JingleBell => vec2(64., 64.),
        }
    }
    pub fn svg(self) -> &'static str {
        match self {
            Self::ComedyMask => include_str!("../../../../assets/bosses/comedy-mask.svg"),
            Self::CastBell => include_str!("../../../../assets/bosses/cast-bell.svg"),
            Self::JingleBell => include_str!("../../../../assets/bosses/jingle-bell.svg"),
        }
    }
    pub fn png(self) -> &'static [u8] {
        match self {
            Self::ComedyMask => include_bytes!("../../../../assets/bosses/comedy-mask.png"),
            Self::CastBell => include_bytes!("../../../../assets/bosses/cast-bell.png"),
            Self::JingleBell => include_bytes!("../../../../assets/bosses/jingle-bell.png"),
        }
    }
    pub(super) fn place(
        self,
        center: Vec2,
        height: f32,
        angle: f32,
        emit: &mut impl FnMut(Primitive),
    ) {
        let y = Vec2::from_angle(angle).perp() * height * 0.5;
        let x = -y.perp() * (self.size().x / self.size().y);
        emit(Primitive::Artwork {
            vertices: [
                center - x - y,
                center + x - y,
                center + x + y,
                center - x + y,
            ],
            art: self,
        });
    }
}
