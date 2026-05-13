//! ServoのソフトウェアGLコンテキスト初期化スモーク（`Servo`本体は起動しない）。
//!
//! ```text
//! cargo run --example servo_smoke -p momaku
//! ```
//!
//! Windowsでは**Visual Studio C++ ビルドツール**が必要です。

use anyhow::Result;
use dpi::PhysicalSize;
use servo::{RenderingContext, SoftwareRenderingContext};

fn main() -> Result<()> {
    let ctx = SoftwareRenderingContext::new(PhysicalSize::new(320, 240))
        .map_err(|e| anyhow::anyhow!("SoftwareRenderingContext::new: {e:?}"))?;
    ctx.make_current()
        .map_err(|e| anyhow::anyhow!("make_current: {e:?}"))?;
    println!("servo_smoke: SoftwareRenderingContext OK");
    Ok(())
}
