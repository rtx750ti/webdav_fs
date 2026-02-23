/// 内部导出的模块
mod internal;

#[cfg(test)]
mod tests;

/// 导出核心入口函数
pub use internal::entrance::lcoal::*;
pub use internal::entrance::remote::*;

pub mod auth {
    use crate::internal;
    pub use internal::auth::*;
    pub use internal::auth::structs::webdav_auth::WebdavAuth;
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
    // 结构体模型
    pub use internal::remote_file::structs::remote_file::*;
    pub use internal::remote_file::structs::remote_file_data::*;
    // 下载器：类型与入口（以 lib 为中心，此处统一导出）
    pub use internal::remote_file::downloader::structs::*;
    pub use internal::remote_file::downloader::impl_traits::*;
    pub use internal::remote_file::downloader::traits::*;
}

pub mod local_file {
    use crate::internal;
    pub use internal::local_file::*;
}
