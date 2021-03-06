use crate::errors::*;
use diesel::prelude::*;
use diesel::pg::PgConnection;
use diesel::sql_types::BigInt;
use diesel_full_text_search::{plainto_tsquery, TsQueryExtensions};
use crate::schema::*;
use std::time::SystemTime;


#[derive(AsChangeset, Serialize, Deserialize, Queryable, Insertable)]
#[table_name="auth_tokens"]
pub struct AuthToken {
    pub id: String,
    pub author: String,
    pub access_token: String,
}

impl AuthToken {
    pub fn create(auth_token: &AuthToken, connection: &PgConnection) -> Result<()> {
        diesel::insert_into(auth_tokens::table)
            .values(auth_token)
            .execute(connection)?;
        Ok(())
    }

    pub fn read(id: &str, connection: &PgConnection) -> Result<AuthToken> {
        auth_tokens::table.find(id)
            .first::<AuthToken>(connection)
            .map_err(Error::from)
    }

    pub fn read_opt(id: &str, connection: &PgConnection) -> Result<Option<AuthToken>> {
        auth_tokens::table.find(id)
            .first::<AuthToken>(connection)
            .optional()
            .map_err(Error::from)
    }

    pub fn delete(id: &str, connection: &PgConnection) -> Result<()> {
        diesel::delete(auth_tokens::table.find(id))
            .execute(connection)?;
        Ok(())
    }
}

/// Make sure we never select search_vector
type AllModuleColumns = (
    modules::id,
    modules::author,
    modules::name,
    modules::description,
    modules::latest,
    modules::featured,
);

pub const ALL_MODULE_COLUMNS: AllModuleColumns = (
    modules::id,
    modules::author,
    modules::name,
    modules::description,
    modules::latest,
    modules::featured,
);

#[derive(AsChangeset, Identifiable, Queryable, Serialize, PartialEq, Debug)]
#[table_name="modules"]
pub struct Module {
    pub id: i32,
    pub author: String,
    pub name: String,
    pub description: String,
    pub latest: Option<String>,
    pub featured: bool,
}

impl Module {
    pub fn create(module: &NewModule, connection: &PgConnection) -> Result<Module> {
        diesel::insert_into(modules::table)
            .values(module)
            .returning(ALL_MODULE_COLUMNS)
            .get_result(connection)
            .map_err(Error::from)
    }

    pub fn find(author: &str, name: &str, connection: &PgConnection) -> Result<Module> {
        modules::table.filter(modules::columns::author.eq(author))
                        .filter(modules::columns::name.eq(name))
                        .select(ALL_MODULE_COLUMNS)
                        .first::<Self>(connection)
                        .map_err(Error::from)
    }

    pub fn find_opt(author: &str, name: &str, connection: &PgConnection) -> Result<Option<Module>> {
        modules::table.filter(modules::columns::author.eq(author))
                        .filter(modules::columns::name.eq(name))
                        .select(ALL_MODULE_COLUMNS)
                        .first::<Self>(connection)
                        .optional()
                        .map_err(Error::from)
    }

    pub fn update_or_create(author: &str, name: &str, description: &str, connection: &PgConnection) -> Result<Module> {
        match Self::find_opt(author, name, connection)? {
            Some(module) => diesel::update(modules::table.filter(modules::columns::id.eq(module.id)))
                            .set(modules::columns::description.eq(description))
                            .returning(ALL_MODULE_COLUMNS)
                            .get_result(connection)
                            .map_err(Error::from),
            None => Self::create(&NewModule {
                author,
                name,
                description,
                latest: None,
            }, connection),
        }
    }

    pub fn id(id: i32, connection: &PgConnection) -> Result<Module> {
        modules::table.find(id)
            .select(ALL_MODULE_COLUMNS)
            .first::<Module>(connection)
            .map_err(Error::from)
    }

    pub fn id_opt(id: i32, connection: &PgConnection) -> Result<Option<Module>> {
        modules::table.find(id)
            .select(ALL_MODULE_COLUMNS)
            .first::<Module>(connection)
            .optional()
            .map_err(Error::from)
    }

    pub fn delete(id: i32, connection: &PgConnection) -> Result<()> {
        diesel::delete(modules::table.find(id))
            .execute(connection)?;
        Ok(())
    }

    pub fn add_version(&self, version: &str, code: &str, connection: &PgConnection) -> Result<()> {
        let _release = Release::create(&NewRelease {
            module_id: self.id,
            version,
            code,
        }, connection)?;

        diesel::update(modules::table.filter(modules::columns::id.eq(self.id)))
            .set(modules::columns::latest.eq(version))
            .execute(connection)?;

        Ok(())
    }

    pub fn search(query: &str, connection: &PgConnection) -> Result<Vec<(Module, i64)>> {
        let q = plainto_tsquery(query);

        let x: Vec<(i32, String, String, String, Option<String>, bool, i64)> = modules::table.select((
                modules::id,
                modules::author,
                modules::name,
                modules::description,
                modules::latest,
                modules::featured,
                diesel::dsl::sql::<BigInt>("sum(releases.downloads) AS sum"),
            ))
            .left_join(releases::table)
            .group_by(modules::id)
            .filter(q.matches(modules::search_vector))
            .order((
                modules::featured.desc(),
                diesel::dsl::sql::<BigInt>("sum").desc(),
            ))
            .load(connection)?;

        Ok(x.into_iter().map(|(id, author, name, description, latest, featured, downloads)| (
            Module {
                id,
                author,
                name,
                description,
                latest,
                featured,
            },
            downloads,
        )).collect())
    }

    pub fn quickstart(connection: &PgConnection) -> Result<Vec<Module>> {
        modules::table
            .select(ALL_MODULE_COLUMNS)
            .filter(modules::featured)
            .order((
                modules::author.asc(),
                modules::name.asc(),
            ))
            .load(connection)
            .map_err(Error::from)
    }
}

#[derive(Insertable)]
#[table_name="modules"]
pub struct NewModule<'a> {
    author: &'a str,
    name: &'a str,
    description: &'a str,
    latest: Option<&'a str>,
}

#[derive(AsChangeset, Identifiable, Queryable, Associations, Serialize, PartialEq, Debug)]
#[belongs_to(Module)]
#[table_name="releases"]
pub struct Release {
    pub id: i32,
    pub module_id: i32,
    pub version: String,
    pub downloads: i32,
    pub code: String,
    pub published: SystemTime,
}

impl Release {
    pub fn create(release: &NewRelease, connection: &PgConnection) -> Result<Release> {
        diesel::insert_into(releases::table)
            .values(release)
            .get_result(connection)
            .map_err(Error::from)
        /*
        releases::table.filter(releases::columns::module_id.eq(release.module_id))
                        .filter(releases::columns::version.eq(&release.version))
                        .select(releases::columns::id)
                        .first::<i32>(connection)
                        .map_err(Error::from)
        */
    }

    pub fn find(module_id: i32, version: &str, connection: &PgConnection) -> Result<Release> {
        releases::table.filter(releases::columns::module_id.eq(module_id))
                        .filter(releases::columns::version.eq(version))
                        .first::<Release>(connection)
                        .map_err(Error::from)
    }

    pub fn try_find(module_id: i32, version: &str, connection: &PgConnection) -> Result<Option<Release>> {
        releases::table.filter(releases::columns::module_id.eq(module_id))
                        .filter(releases::columns::version.eq(version))
                        .first::<Release>(connection)
                        .optional()
                        .map_err(Error::from)
    }

    pub fn id(id: i32, connection: &PgConnection) -> Result<Release> {
        releases::table.find(id)
            .first::<Release>(connection)
            .map_err(Error::from)
    }

    pub fn id_opt(id: i32, connection: &PgConnection) -> Result<Option<Release>> {
        releases::table.find(id)
            .first::<Release>(connection)
            .optional()
            .map_err(Error::from)
    }

    pub fn delete(id: i32, connection: &PgConnection) -> Result<()> {
        diesel::delete(releases::table.find(id))
            .execute(connection)?;
        Ok(())
    }

    pub fn bump_downloads(&self, connection: &PgConnection) -> Result<()> {
        diesel::update(releases::table.filter(releases::id.eq(self.id)))
            .set(releases::downloads.eq(releases::downloads + 1))
            .execute(connection)?;
        Ok(())
    }

    pub fn latest(connection: &PgConnection) -> Result<Option<Release>> {
        releases::table
            .order_by(releases::published.desc())
            .first::<Release>(connection)
            .optional()
            .map_err(Error::from)
    }
}

#[derive(Insertable)]
#[table_name="releases"]
pub struct NewRelease<'a> {
    module_id: i32,
    version: &'a str,
    code: &'a str,
}
