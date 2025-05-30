use crate::error::Error;
use crate::{Id, Result};
use sqlx::sqlite::SqliteRow;
use sqlx::{Pool, Sqlite};

pub struct SqlDatabase {
    pub(crate) pool: Pool<Sqlite>,
}

impl SqlDatabase {
    pub async fn create_table(
        &mut self,
        name: &str,
        entries: Vec<&str>,
        indexes: Vec<&str>,
    ) -> Result<()> {
        if self
            .select_from_where_equals("sqlite_master ", "name", name)
            .await?
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
            sqlx::query(
                format!(
                    "CREATE INDEX {}_index_{} ON {} ({})",
                    index, name, name, index
                )
                .as_str(),
            )
            .execute(&self.pool)
            .await
            .map_err(|err| Error::DbCreateTableError(name.to_string(), err))?;
        }
        Ok(())
    }
    pub async fn select_from_where_equals(
        &mut self,
        table_name: &str,
        column_name: &str,
        value: &str,
    ) -> Result<Vec<SqliteRow>> {
        let rows =
            sqlx::query(format!("SELECT * FROM {} WHERE {}=?", table_name, column_name).as_str())
                .bind(value)
                .fetch_all(&self.pool)
                .await
                .map_err(|err| {
                    Error::DbSelectFromWhereError(
                        table_name.to_string(),
                        format!("{}={}", column_name, value),
                        err,
                    )
                })?;

        Ok(rows)
    }

    pub async fn select_from_joined_where_equals(
        &mut self,
        select: Vec<&str>,
        first_table_name: &str,
        second_table_name: &str,
        join_left: &str,
        join_right: &str,
        where_column_name: &str,
        where_value_name: &str,
    ) -> Result<Vec<SqliteRow>> {
        let mut select_part = "".to_string();

        for select_item in select {
            select_part += select_item;
            select_part += ",";
        }

        if select_part.is_empty() {
            select_part = '*'.to_string();
        } else {
            select_part = select_part.strip_suffix(",").unwrap().to_string();
        }

        let rows = sqlx::query(
            format!(
                "SELECT {} FROM {} INNER JOIN {} ON {} = {} WHERE {}=?",
                select_part,
                first_table_name,
                second_table_name,
                join_left,
                join_right,
                where_column_name
            )
            .as_str(),
        )
        .bind(where_value_name)
        .fetch_all(&self.pool)
        .await
        .map_err(|err| {
            Error::DbSelectFromJoinedIdsError(
                first_table_name.to_string(),
                second_table_name.to_string(),
                format!("{}={}", where_column_name, where_value_name),
                err,
            )
        })?;

        Ok(rows)
    }

    fn vec_to_insert_str(
        table_name: &str,
        values: Vec<Vec<String>>,
        upserts: Vec<(&str, &str)>,
    ) -> String {
        let mut insert_sql_str = format!("INSERT INTO {} VALUES ", table_name);

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

        insert_sql_str += "ON CONFLICT(id) DO UPDATE SET ";

        for upsert in upserts {
            insert_sql_str += upsert.0;
            insert_sql_str += "=";
            insert_sql_str += "excluded.";
            insert_sql_str += upsert.1;
            insert_sql_str += ",";
        }

        insert_sql_str.strip_suffix(",").unwrap().to_string()
    }

    pub async fn max_in(&mut self, table_name: &str, column_name: &str) -> Result<Option<Id>> {
        let result: sqlx::Result<i64> =
            sqlx::query_scalar(format!("SELECT MAX({}) FROM {}", column_name, table_name).as_str())
                .fetch_one(&self.pool)
                .await;

        match result {
            Err(sqlx::Error::RowNotFound) => Ok(None),
            Err(err) => Err(Error::DbLastIdError(err)),
            Ok(id) => Ok(Some(id as u32)),
        }
    }

    pub async fn insert_row_into(
        &mut self,
        table_name: &str,
        row: Vec<String>,
        upserts: Vec<(&str, &str)>,
    ) -> Result<()> {
        let insert_sql_str = Self::vec_to_insert_str(table_name, vec![row], upserts);

        sqlx::query(&insert_sql_str)
            .execute(&self.pool)
            .await
            .map_err(|err| Error::SqlDbInsertError(insert_sql_str, err))?;

        Ok(())
    }

    pub async fn insert_rows_into(
        &mut self,
        table_name: &str,
        values: Vec<Vec<String>>,
        upserts: Vec<(&str, &str)>,
    ) -> Result<()> {
        let insert_sql_str = Self::vec_to_insert_str(table_name, values, upserts);

        sqlx::query(&insert_sql_str)
            .execute(&self.pool)
            .await
            .map_err(|err| Error::SqlDbInsertError(insert_sql_str, err))?;

        Ok(())
    }
}
