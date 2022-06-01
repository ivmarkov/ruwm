#[macro_export]
#[allow(unused_macros)]
macro_rules! check {
    ($result:expr) => {
        match $result {
            Ok(value) => Ok(value),
            Err(err) => {
                log::error!("Failed: {:?}", err);
                Err(err)
            }
        }
    };
}

#[allow(unused_imports)]
pub use check;
