pub mod records;      // file record & filtering hooks
pub mod settings;     // UI settings hook
pub mod scan;         // scan channel draining logic
pub mod blocks;       // grouping hooks (filters/layout/etc.)

pub use records::*;
pub use settings::*;
pub use scan::*;
pub use blocks::*;
