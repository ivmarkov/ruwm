use core::fmt::{self, Display, Formatter};

#[derive(Debug)]
pub enum EitherError<E1, E2> {
    E1(E1),
    E2(E2),
}

impl<E1, E2> Display for EitherError<E2, E1>
where
    E1: Display,
    E2: Display,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::E1(e) => write!(f, "E1: {}", e),
            Self::E2(e) => write!(f, "E2: {}", e),
        }
    }
}

// impl<E1, E2> Error for EitherError<E2, E1>
// where
//     E1: Error,
//     E2: Error,
// {
//     fn kind(&self) -> ErrorKind {
//         match self {
//             Self::E1(e) => e.kind(),
//             Self::E2(e) => e.kind(),
//         }
//     }
// }

#[cfg(feature = "std")]
impl<E1, E2> std::error::Error for EitherError<E1, E2>
where
    E1: Display + Debug,
    E2: Display + Debug,
{
}

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
