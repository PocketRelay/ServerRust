//! SeaORM Entity. Generated by sea-orm-codegen 0.9.3

use chrono::NaiveDateTime;
use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "galaxy_at_war")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: u32,
    pub player_id: u32,
    pub last_modified: NaiveDateTime,
    pub group_a: u16,
    pub group_b: u16,
    pub group_c: u16,
    pub group_d: u16,
    pub group_e: u16,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "Entity",
        from = "Column::Id",
        to = "Column::PlayerId",
        on_update = "NoAction",
        on_delete = "NoAction"
    )]
    SelfRef,
    #[sea_orm(
    belongs_to = "super::players::Entity",
    from = "Column::PlayerId",
    to = "super::players::Column::Id"
    )]
    Player,
}

impl Related<super::players::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Player.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
