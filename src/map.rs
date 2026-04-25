use walkers::sources::{Attribution, TileSource};
use walkers::TileId;

#[derive(PartialEq, Clone, Copy, Default)]
pub(crate) enum MapLayer {
    #[default]
    Satellite,
    Streets,
    Topo,
}

impl MapLayer {
    pub(crate) fn label(self) -> &'static str {
        match self {
            MapLayer::Satellite => "Satellite",
            MapLayer::Streets => "Streets",
            MapLayer::Topo => "Topo",
        }
    }
}

pub(crate) struct LayerSource(pub(crate) MapLayer);

impl TileSource for LayerSource {
    fn tile_url(&self, t: TileId) -> String {
        match self.0 {
            MapLayer::Satellite => format!(
                "https://server.arcgisonline.com/ArcGIS/rest/services/World_Imagery/MapServer/tile/{}/{}/{}",
                t.zoom, t.y, t.x
            ),
            MapLayer::Streets => format!(
                "https://tile.openstreetmap.org/{}/{}/{}.png",
                t.zoom, t.x, t.y
            ),
            MapLayer::Topo => format!(
                "https://tile.opentopomap.org/{}/{}/{}.png",
                t.zoom, t.x, t.y
            ),
        }
    }

    fn attribution(&self) -> Attribution {
        match self.0 {
            MapLayer::Satellite => Attribution {
                text: "© Esri, DigitalGlobe, GeoEye, Earthstar Geographics",
                url: "https://www.esri.com/",
                logo_light: None,
                logo_dark: None,
            },
            MapLayer::Streets => Attribution {
                text: "© OpenStreetMap contributors",
                url: "https://www.openstreetmap.org/copyright",
                logo_light: None,
                logo_dark: None,
            },
            MapLayer::Topo => Attribution {
                text: "© OpenTopoMap contributors",
                url: "https://opentopomap.org/",
                logo_light: None,
                logo_dark: None,
            },
        }
    }
}
