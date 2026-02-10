/// 内部导出的模块
mod internal;

/// 导出核心入口函数
pub use internal::entrance::lcoal::*;
pub use internal::entrance::remote::*;

pub mod auth {
    use crate::internal;
    pub use internal::auth::*;
}

/// 对外提供webdav基础访问能力，不能限制死在入口函数中，以防有人自己要用
pub mod webdav {
    pub mod functions {
        use crate::internal;
        pub use internal::webdav::functions::*;
    }

    pub mod enums {
        use crate::internal;
        pub use internal::webdav::enums::*;
    }

    pub use crate::internal::webdav::raw_file_xml::*;
}

pub mod states {
    pub mod lock_reactive {
        use crate::internal;
        pub use internal::states::lock_reactive::*;
    }

    pub mod unlock_reactive {
        use crate::internal;
        pub use internal::states::unlock_reactive::*;
    }
}

pub mod remote_file {
    use crate::internal;
    pub use internal::remote_file::*;
}

pub mod local_file {
    use crate::internal;
    pub use internal::local_file::*;
}
