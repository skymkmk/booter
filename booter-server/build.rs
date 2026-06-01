use std::process::Command;
use std::path::Path;

fn main() {
    // 告诉 Cargo，如果这些前端核心文件发生变化，则重新运行此构建脚本
    println!("cargo:rerun-if-changed=../booter-web/src");
    println!("cargo:rerun-if-changed=../booter-web/package.json");
    println!("cargo:rerun-if-changed=../booter-web/index.html");
    println!("cargo:rerun-if-changed=../booter-web/vite.config.ts");

    let web_dir = Path::new("../booter-web");

    // 尝试运行 bun run build
    // 针对 Windows 环境，通常需要用到 cmd /c 或者直接调用 bun.cmd
    let status = if cfg!(target_os = "windows") {
        Command::new("cmd")
            .args(&["/C", "bun run build"])
            .current_dir(web_dir)
            .status()
    } else {
        Command::new("bun")
            .arg("run")
            .arg("build")
            .current_dir(web_dir)
            .status()
    };

    match status {
        Ok(s) if s.success() => {
            // 前端构建成功，继续后端的编译
        }
        Ok(s) => {
            panic!("Frontend build failed with exit code: {}", s);
        }
        Err(e) => {
            panic!("Failed to execute frontend build command (is bun installed?): {}", e);
        }
    }
}
