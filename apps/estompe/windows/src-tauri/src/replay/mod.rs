#[cfg_attr(not(all(feature = "instant-replay", windows)), allow(dead_code))]
mod types;

#[cfg(feature = "instant-replay")]
#[cfg_attr(not(windows), allow(dead_code))]
mod automate;
#[cfg(feature = "instant-replay")]
mod avec;
#[cfg(feature = "instant-replay")]
#[cfg_attr(not(windows), allow(dead_code))]
mod cadence;
#[cfg(feature = "instant-replay")]
#[cfg_attr(not(windows), allow(dead_code))]
mod ecriture;
#[cfg(feature = "instant-replay")]
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) mod galerie;
#[cfg(feature = "instant-replay")]
#[cfg_attr(not(windows), allow(dead_code))]
mod garde;
#[cfg(feature = "instant-replay")]
#[cfg_attr(not(windows), allow(dead_code))]
mod micro;
#[cfg(feature = "instant-replay")]
mod montage;
#[cfg(feature = "instant-replay")]
#[cfg_attr(not(windows), allow(dead_code))]
mod mp4;
#[cfg(feature = "instant-replay")]
#[cfg_attr(not(windows), allow(dead_code))]
mod reglages;
#[cfg(not(feature = "instant-replay"))]
mod sans;
#[cfg(feature = "instant-replay")]
#[cfg_attr(not(windows), allow(dead_code))]
mod tampon;
#[cfg(all(feature = "instant-replay", windows))]
mod win;

#[cfg(all(feature = "instant-replay", windows))]
pub use win::essai_encodeur;

#[cfg(feature = "instant-replay")]
pub use avec::*;
#[cfg(feature = "instant-replay")]
pub use montage::*;
#[cfg(not(feature = "instant-replay"))]
pub use sans::*;
