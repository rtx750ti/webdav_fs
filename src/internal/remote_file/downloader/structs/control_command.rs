/// 下载控制命令（通过 QueueReactiveProperty 传递，FIFO 保证顺序）
#[derive(Debug, Clone)]
pub enum ControlCommand {
    Pause,
    Resume,
    Cancel,
}

