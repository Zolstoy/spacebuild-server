use crate::error::Error;
use crate::Result;
use sqlx::sqlite::SqliteRow;
use sqlx::{Pool, Sqlite};

pub struct SqlDb {
    pool: Pool<Sqlite>,
}

impl SqlDb {
    pub fn new(pool: Pool<Sqlite>) -> SqlDb {
        SqlDb { pool }
    }

    pub async fn create_table(&mut self, name: &str, entries: Vec<&str>, indexes: Vec<&str>) -> Result<()> {
        if self
            .select_from_where_equals("sqlite_master ", "name", name)
            .await
            .len()
            > 0
        {
            return Ok(());
        }

        let mut sql_str = format!("CREATE TABLE {} (", name);
        for entry in entries {
            sql_str += format!("{},", entry).as_str();
        }
        sql_str = sql_str.strip_suffix(",").unwrap().to_string();
        sql_str += ");";

        sqlx::query(&sql_str)
            .execute(&self.pool)
            .await
            .map_err(|err| Error::DbCreateTableError(name.to_string(), err))?;

        for index in indexes {
            sqlx::query(format!("CREATE INDEX {}_index_{} ON {} ({})", index, name, name, index).as_str())
                .execute(&self.pool)
                .await
                .map_err(|err| Error::DbCreateTableError(name.to_string(), err))?;
        }
        Ok(())
    }
    pub async fn select_from_where_equals(&self, table_name: &str, column_name: &str, value: &str) -> Vec<SqliteRow> {
        sqlx::query(format!("SELECT * FROM {} WHERE {}=?", table_name, column_name).as_str())
            .bind(value)
            .fetch_all(&self.pool)
            .await
            .unwrap()
    }

    pub async fn select_from_where_like(&self, table_name: &str, column_name: &str, value: &str) -> Vec<SqliteRow> {
        sqlx::query(format!("SELECT * FROM {} WHERE {} LIKE ?", table_name, column_name).as_str())
            .bind(value)
            .fetch_all(&self.pool)
            .await
            .unwrap()
    }

    fn vec_to_insert_str(
        table_name: &str,
        columns: Option<Vec<String>>,
        values: Vec<Vec<String>>,
        upserts: Vec<(&str, &str)>,
    ) -> String {
        let mut insert_sql_str = format!("INSERT INTO {} ", table_name);

        if columns.is_some() {
            let columns = columns.unwrap();
            if !columns.is_empty() {
                insert_sql_str += "(";
                for column in columns {
                    insert_sql_str += format!("{}, ", column).as_str();
                }
                insert_sql_str = insert_sql_str.strip_suffix(", ").unwrap().to_string();
                insert_sql_str += ") ";
            }
        }

        insert_sql_str += "VALUES ";

        for line in values {
            insert_sql_str += "(";
            for value in line {
                insert_sql_str += value.as_str();
                insert_sql_str += ",";
            }
            insert_sql_str = insert_sql_str.strip_suffix(",").unwrap().to_string();
            insert_sql_str += "),";
        }

        insert_sql_str = insert_sql_str.strip_suffix(",").unwrap().to_string();

        if !upserts.is_empty() {
            insert_sql_str += "ON CONFLICT(id) DO UPDATE SET ";

            for upsert in upserts {
                insert_sql_str += upsert.0;
                insert_sql_str += "=";
                insert_sql_str += "excluded.";
                insert_sql_str += upsert.1;
                insert_sql_str += ",";
            }

            insert_sql_str.strip_suffix(",").unwrap().to_string()
        } else {
            insert_sql_str
        }
    }

    pub async fn insert_row_into(
        &mut self,
        table_name: &str,
        columns: Option<Vec<String>>,
        row: Vec<String>,
        upserts: Vec<(&str, &str)>,
    ) -> u32 {
        let insert_sql_str = Self::vec_to_insert_str(table_name, columns, vec![row], upserts);

        sqlx::query(&insert_sql_str)
            .execute(&self.pool)
            .await
            .unwrap()
            .last_insert_rowid() as u32
    }

    pub async fn insert_rows_into(
        &self,
        table_name: &str,
        columns: Option<Vec<String>>,
        values: Vec<Vec<String>>,
        upserts: Vec<(&str, &str)>,
    ) -> u32 {
        sqlx::query(&Self::vec_to_insert_str(table_name, columns, values, upserts))
            .execute(&self.pool)
            .await
            .unwrap()
            .last_insert_rowid() as u32
    }
}
