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
        pub use internal::webdav::functions::get_folders_raw_data::*;
    }

    pub mod enums {
        use crate::internal;
        pub use internal::webdav::enums::*;
    }

    pub mod traits {
        pub use crate::internal::webdav::raw_xml::impl_multi_status::*;
    }

    pub mod structs {
        pub use crate::internal::webdav::raw_xml::raw_file::*;
    }
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
    // 导出结构体模型
    pub use internal::remote_file::structs::remote_file::*;
    pub use internal::remote_file::structs::remote_file_data::*;
    // 导出公用函数
}

pub mod local_file {
    use crate::internal;
    pub use internal::local_file::*;
}
