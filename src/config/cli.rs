use clap::Parser;
/// 一个简单的异步 DNS 服务器
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
pub struct Cli {
    /// 指定配置文件的路径
    #[arg(short = 'f', long, default_value = "config.toml")]
    pub config_file: String,
}
