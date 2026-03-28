use std::net::{IpAddr, Ipv4Addr};

use fake::{Fake, Faker};
use sqlx::PgPool;
use sqlx::types::ipnetwork::IpNetwork;

use rand::RngExt;
use tachikoma::db::models::{GroupId, HostId, UserId};
use uuid::Uuid;

pub struct Generator {
    pub pool: PgPool,
}
pub struct MockHost {
    pub id: HostId,
    pub hostname: String,
    pub ip: Ipv4Addr,
}
pub struct MockGroup {
    pub id: GroupId,
    pub name: String,
}
pub struct MockUser {
    pub id: UserId,
    pub tg_handle: String,
}

impl Generator {
    pub async fn generate_group(&mut self) -> MockGroup {
        let name = Uuid::new_v4().to_string();
        let row = sqlx::query!("INSERT INTO groups (name) VALUES ($1) RETURNING id", name)
            .fetch_one(&self.pool)
            .await
            .unwrap();
        MockGroup {
            id: row.id.into(),
            name,
        }
    }
    pub async fn generate_host(&mut self) -> MockHost {
        self.generate_host_in_group(&GroupId(0)).await
    }

    pub async fn generate_host_in_group(&mut self, group_id: &GroupId) -> MockHost {
        let hostname = Uuid::new_v4().to_string();
        let ip: Ipv4Addr = Faker.fake();
        let net = IpNetwork::new(IpAddr::V4(ip), 32).unwrap();
        let row = sqlx::query!(
            "INSERT INTO hosts (hostname, ip_address, group_id) VALUES ($1, $2, $3) RETURNING id",
            hostname,
            net,
            group_id.clone().0,
        )
        .fetch_one(&self.pool)
        .await
        .unwrap();
        MockHost {
            id: row.id.into(),
            hostname,
            ip,
        }
    }
    pub async fn generate_user(&mut self) -> MockUser {
        let mut rng = rand::rng();

        let mail = Uuid::new_v4().to_string();
        let dn = mail.clone();
        let tg_handle = rng.random::<u32>().to_string();

        let row = sqlx::query!(
            "INSERT INTO users (email, tg_handle, dn) VALUES ($1, $2, $3) RETURNING id",
            mail,
            tg_handle,
            dn,
        )
        .fetch_one(&self.pool)
        .await
        .unwrap();

        MockUser {
            id: row.id.into(),
            tg_handle,
        }
    }
}
