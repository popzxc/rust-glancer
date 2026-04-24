mod utils;

use expect_test::expect;

use self::utils::check_project_semantic_ir;

#[test]
fn dumps_semantic_ir_signatures() {
    check_project_semantic_ir(
        r#"
//- /Cargo.toml
[package]
name = "semantic_fixture"
version = "0.1.0"
edition = "2024"

//- /src/lib.rs
pub struct User<T> {
    pub id: UserId,
    payload: Option<T>,
}

pub struct UserId(u64);

pub enum LoadState<E> {
    Empty,
    Loaded(User),
    Failed { error: E },
}

pub trait Repository<T>
where
    T: Clone,
{
    type Error;
    const KIND: &'static str;
    fn get(&self, id: UserId) -> Result<T, Self::Error>;
}

pub struct DbRepository<T>(T);

impl<T> Repository<T> for DbRepository<T>
where
    T: Clone,
{
    type Error = DbError;
    const KIND: &'static str = "db";
    fn get(&self, id: UserId) -> Result<T, DbError> {
        todo!()
    }
}

pub struct DbError;

pub type UserResult<T> = Result<User<T>, DbError>;
pub const DEFAULT_ID: UserId = UserId(0);
pub static mut CACHE_READY: bool = false;
"#,
        expect![[r#"
            package semantic_fixture

            semantic_fixture [lib]
            crate
            - pub struct User<T>
              - pub field id: UserId
              - field payload: Option<T>
            - pub struct UserId
              - field #0: u64
            - pub enum LoadState<E>
              - variant Empty
              - variant Loaded
                - field #0: User
              - variant Failed
                - field error: E
            - pub trait Repository<T> where T: Clone
              - type Error
              - const KIND: &'static str
              - fn get(&self, id: UserId) -> Result<T, Self::Error>
            - pub struct DbRepository<T>
              - field #0: T
            - pub struct DbError
            - pub type UserResult<T> = Result<User<T>, DbError>
            - pub const DEFAULT_ID: UserId
            - pub static mut CACHE_READY: bool
            - impl<T> Repository<T> for DbRepository<T> where T: Clone
              - type Error = DbError
              - const KIND: &'static str
              - fn get(&self, id: UserId) -> Result<T, DbError>
        "#]],
    );
}

#[test]
fn preserves_absolute_type_path_prefixes() {
    check_project_semantic_ir(
        r#"
//- /Cargo.toml
[package]
name = "absolute_type_fixture"
version = "0.1.0"
edition = "2024"

//- /src/lib.rs
pub struct Root;
pub struct UsesAbsolute(::absolute_type_fixture::Root);
pub type AbsoluteAlias = ::absolute_type_fixture::Root;
"#,
        expect![[r#"
            package absolute_type_fixture

            absolute_type_fixture [lib]
            crate
            - pub struct Root
            - pub struct UsesAbsolute
              - field #0: ::absolute_type_fixture::Root
            - pub type AbsoluteAlias = ::absolute_type_fixture::Root
        "#]],
    );
}
