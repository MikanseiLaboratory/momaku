//! Servo のソフトウェア GL コンテキスト初期化スモーク（`Servo` 本体は起動しない）。
//!
//! ```text
//! cargo run --example servo_smoke -p momaku
//! ```
//!
//! Windows では **Visual Studio C++ ビルドツール** が必要です。

use dpi::PhysicalSize;
use servo::SoftwareRenderingContext;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let ctx = SoftwareRenderingContext::new(PhysicalSize::new(320, 240))?;
    ctx.make_current()?;
    println!("servo_smoke: SoftwareRenderingContext OK");
    Ok(())
}
