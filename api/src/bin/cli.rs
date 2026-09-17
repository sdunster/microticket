//! microticket `cli` — a thin, ergonomic wrapper over the DB API, following
//! seslogin's `bin/cli.rs` conventions (one subcommand tree per object type,
//! writes happen immediately, `--dry-run` reports what *would* change
//! instead). This is how an operator bootstraps the first organisation and
//! its first owner — see `DEVELOPMENT.md`'s "Bootstrapping the first
//! organisation" section for the sequence this drives.
//!
//! Unlike `bin/local-tables.rs`/`bin/local-seed.rs`, this CLI is **not**
//! restricted to a local DynamoDB endpoint — it is the general admin tool,
//! meant to run against a real deployed `DB_PREFIX` too.

use anyhow::{Context, Result, anyhow};
use clap::{Parser, Subcommand};
use microticket::db::{Handler, Instance, InstanceUpdateShape, MembershipRole, User};
use microticket::dynamodb;
use microticket::inbound::routing;

#[derive(Parser, Debug)]
#[command(about = "Admin/inspection CLI for the microticket DB API")]
struct Cli {
    /// DynamoDB table prefix (e.g. "prod", "local"). Falls back to the
    /// DB_PREFIX env var.
    #[arg(long, global = true)]
    db_prefix: Option<String>,

    /// Report what the command would change without writing anything.
    #[arg(long, global = true, default_value_t = false)]
    dry_run: bool,

    #[command(subcommand)]
    object: Object,
}

#[derive(Subcommand, Debug)]
enum Object {
    /// Tenant organisations.
    Instance {
        #[command(subcommand)]
        cmd: InstanceCmd,
    },
    /// Inbound email addresses mapped to an instance.
    Address {
        #[command(subcommand)]
        cmd: AddressCmd,
    },
    /// System users (people who can log in).
    User {
        #[command(subcommand)]
        cmd: UserCmd,
    },
    /// Instance memberships — who belongs to which instance, in what role.
    /// This is how a member is added; there is no invite mutation over
    /// GraphQL (see the build plan's "member invites are deferred").
    Member {
        #[command(subcommand)]
        cmd: MemberCmd,
    },
}

#[derive(Subcommand, Debug)]
enum InstanceCmd {
    /// Create an instance. Fails if the slug is already taken (checked, then
    /// written — see `db::Handler::create_instance`'s doc comment for the
    /// narrow race this leaves open, which is fine for an operator-driven,
    /// low-frequency action like this one).
    Create {
        name: String,
        slug: String,
        /// From: display name for outbound mail. Defaults to `name`.
        #[arg(long)]
        from_name: Option<String>,
        /// Appended to outbound replies. Defaults to empty.
        #[arg(long)]
        signature: Option<String>,
        /// Allow anyone to submit a ticket to this instance from the public
        /// web form, after verifying their email with a code.
        #[arg(long)]
        public_submission_enabled: bool,
    },
    /// List every instance (active and soft-deleted).
    List,
    /// Update an existing instance's fields, or soft-delete/restore it.
    Update {
        id: String,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        from_name: Option<String>,
        #[arg(long)]
        signature: Option<String>,
        #[arg(long)]
        public_submission_enabled: Option<bool>,
        /// Soft-delete (`true`) or restore (`false`) the instance. Mutually
        /// exclusive with the field flags above — deletion is its own
        /// write, matching `db::InstanceUpdateShape`.
        #[arg(long, conflicts_with_all = ["name", "from_name", "signature", "public_submission_enabled"])]
        deleted: Option<bool>,
    },
}

#[derive(Subcommand, Debug)]
enum AddressCmd {
    /// Map an inbound address (or `*@domain` wildcard) to an instance.
    /// Normalized and classified by `inbound::routing::classify_for_storage`
    /// — the same logic the `addInboundAddress` GraphQL mutation uses.
    Add {
        #[arg(long)]
        instance: String,
        address: String,
    },
    /// Remove an inbound address.
    Remove { address: String },
    /// List every inbound address mapped to an instance.
    List {
        #[arg(long)]
        instance: String,
    },
}

#[derive(Subcommand, Debug)]
enum UserCmd {
    /// Create a user. Does not create any membership — pair with `member add`.
    Create { email: String, name: String },
    /// List every user.
    List,
}

#[derive(Subcommand, Debug)]
enum MemberCmd {
    /// Grant a user a role in an instance. `--user` accepts either a user id
    /// or an email (resolved via `email-index`), mirroring
    /// `auth::resolve_dev_auth`'s convention. Fails if the user already has a
    /// membership in that instance — remove it first to change the role.
    Add {
        #[arg(long)]
        instance: String,
        #[arg(long)]
        user: String,
        #[arg(long, default_value = "agent")]
        role: RoleArg,
    },
    /// Revoke a user's membership in an instance.
    Remove {
        #[arg(long)]
        instance: String,
        #[arg(long)]
        user: String,
    },
    /// List every member of an instance and their role.
    List {
        #[arg(long)]
        instance: String,
    },
}

#[derive(clap::ValueEnum, Clone, Copy, Debug)]
enum RoleArg {
    Owner,
    Agent,
}

impl From<RoleArg> for MembershipRole {
    fn from(r: RoleArg) -> Self {
        match r {
            RoleArg::Owner => MembershipRole::Owner,
            RoleArg::Agent => MembershipRole::Agent,
        }
    }
}

fn bool_str(b: bool) -> &'static str {
    if b { "true" } else { "false" }
}

fn opt_str(s: &str) -> String {
    if s.is_empty() {
        "-".to_string()
    } else {
        s.to_string()
    }
}

fn print_instance(i: &Instance) {
    println!(
        "id: {}\nname: {}\nslug: {}\npublic_submission_enabled: {}\nfrom_name: {}\nsignature: {}\ncreated_at: {}\ndeleted: {}",
        i.id,
        i.name,
        i.slug,
        bool_str(i.public_submission_enabled),
        opt_str(&i.from_name),
        opt_str(&i.signature),
        i.created_at,
        bool_str(i.deleted),
    );
}

fn print_user(u: &User) {
    println!(
        "id: {}\nemail: {}\nname: {}\nenabled: {}\ncreated_at: {}\naccess_time: {}",
        u.id,
        u.email,
        u.name,
        bool_str(u.enabled),
        u.created_at,
        u.access_time
            .map(|t| t.to_string())
            .unwrap_or_else(|| "-".to_string()),
    );
}

/// Resolve `--user` to a user id: an email (contains `@`) is looked up via
/// `email-index`; anything else is treated as a literal id (not verified to
/// exist here — the write that follows will fail loudly if it doesn't).
async fn resolve_user_id(db: &impl Handler, id_or_email: &str) -> Result<String> {
    if id_or_email.contains('@') {
        db.get_user_id_by_email(id_or_email)
            .await
            .context("looking up user by email")?
            .ok_or_else(|| anyhow!("no user with email {id_or_email}"))
    } else {
        Ok(id_or_email.to_string())
    }
}

async fn run_instance(db: &impl Handler, cmd: InstanceCmd, dry_run: bool) -> Result<()> {
    match cmd {
        InstanceCmd::Create {
            name,
            slug,
            from_name,
            signature,
            public_submission_enabled,
        } => {
            if db.get_instance_id_by_slug(&slug).await?.is_some() {
                return Err(anyhow!("slug {slug:?} is already taken"));
            }
            let from_name = from_name.unwrap_or_else(|| name.clone());
            let signature = signature.unwrap_or_default();
            if dry_run {
                println!(
                    "[dry-run] would create instance name={name:?} slug={slug:?} \
                     from_name={from_name:?} public_submission_enabled={public_submission_enabled}"
                );
                return Ok(());
            }
            let instance = db
                .create_instance(
                    &name,
                    &slug,
                    &from_name,
                    &signature,
                    public_submission_enabled,
                )
                .await?;
            print_instance(&instance);
        }
        InstanceCmd::List => {
            let instances = db.list_instances().await?;
            println!(
                "{:>12}  {:<20}  {:<20}  {:<7}  {:<7}",
                "id", "name", "slug", "public", "deleted"
            );
            for i in &instances {
                println!(
                    "{:>12}  {:<20}  {:<20}  {:<7}  {:<7}",
                    i.id,
                    i.name,
                    i.slug,
                    bool_str(i.public_submission_enabled),
                    bool_str(i.deleted),
                );
            }
            println!("{} instance(s)", instances.len());
        }
        InstanceCmd::Update {
            id,
            name,
            from_name,
            signature,
            public_submission_enabled,
            deleted,
        } => {
            if let Some(deleted) = deleted {
                if dry_run {
                    println!("[dry-run] would set instance {id} deleted={deleted}");
                    return Ok(());
                }
                db.update_instance(&id, InstanceUpdateShape::SetDeleted(deleted))
                    .await?;
                println!("instance {id} deleted={deleted}");
                return Ok(());
            }
            let current = db
                .get_instances(&[&id])
                .await?
                .into_iter()
                .next()
                .flatten()
                .ok_or_else(|| anyhow!("no instance with id {id}"))?;
            let name = name.unwrap_or(current.name);
            let from_name = from_name.unwrap_or(current.from_name);
            let signature = signature.unwrap_or(current.signature);
            let public_submission_enabled =
                public_submission_enabled.unwrap_or(current.public_submission_enabled);
            if dry_run {
                println!(
                    "[dry-run] would update instance {id}: name={name:?} from_name={from_name:?} \
                     signature={signature:?} public_submission_enabled={public_submission_enabled}"
                );
                return Ok(());
            }
            db.update_instance(
                &id,
                InstanceUpdateShape::Fields {
                    name: &name,
                    from_name: &from_name,
                    signature: &signature,
                    public_submission_enabled,
                },
            )
            .await?;
            println!("updated instance {id}");
        }
    }
    Ok(())
}

async fn run_address(db: &impl Handler, cmd: AddressCmd, dry_run: bool) -> Result<()> {
    match cmd {
        AddressCmd::Add { instance, address } => {
            let (normalized, kind) =
                routing::classify_for_storage(&address).map_err(|e| anyhow!(e))?;
            if let Some(existing) = db.get_inbound_address(&normalized).await? {
                return Err(anyhow!(
                    "{normalized} is already mapped to instance {}",
                    existing.instance_id
                ));
            }
            if dry_run {
                println!(
                    "[dry-run] would add {normalized} ({:?}) to instance {instance}",
                    kind
                );
                return Ok(());
            }
            let created = db
                .create_inbound_address(&normalized, &instance, kind)
                .await?;
            println!(
                "address: {}\ninstance_id: {}\nkind: {:?}\ncreated_at: {}",
                created.address, created.instance_id, created.kind, created.created_at
            );
        }
        AddressCmd::Remove { address } => {
            let normalized = address.trim().to_lowercase();
            let existing = db
                .get_inbound_address(&normalized)
                .await?
                .ok_or_else(|| anyhow!("no inbound address {normalized}"))?;
            if dry_run {
                println!(
                    "[dry-run] would remove {normalized} (instance {})",
                    existing.instance_id
                );
                return Ok(());
            }
            db.delete_inbound_address(&normalized).await?;
            println!("removed {normalized}");
        }
        AddressCmd::List { instance } => {
            let addrs = db.list_inbound_addresses_by_instance(&instance).await?;
            println!("{:<30}  {:<10}  {:<12}", "address", "kind", "created_at");
            for a in &addrs {
                println!("{:<30}  {:?}  {:<12}", a.address, a.kind, a.created_at);
            }
            println!("{} address(es)", addrs.len());
        }
    }
    Ok(())
}

async fn run_user(db: &impl Handler, cmd: UserCmd, dry_run: bool) -> Result<()> {
    match cmd {
        UserCmd::Create { email, name } => {
            if db.get_user_id_by_email(&email).await?.is_some() {
                return Err(anyhow!("a user with email {email} already exists"));
            }
            if dry_run {
                println!("[dry-run] would create user email={email:?} name={name:?}");
                return Ok(());
            }
            let user = db.create_user(&email, &name).await?;
            print_user(&user);
        }
        UserCmd::List => {
            let users = db.list_users().await?;
            println!(
                "{:>12}  {:<30}  {:<20}  {:<7}",
                "id", "email", "name", "enabled"
            );
            for u in &users {
                println!(
                    "{:<12}  {:<30}  {:<20}  {:<7}",
                    u.id,
                    u.email,
                    u.name,
                    bool_str(u.enabled)
                );
            }
            println!("{} user(s)", users.len());
        }
    }
    Ok(())
}

async fn run_member(db: &impl Handler, cmd: MemberCmd, dry_run: bool) -> Result<()> {
    match cmd {
        MemberCmd::Add {
            instance,
            user,
            role,
        } => {
            let user_id = resolve_user_id(db, &user).await?;
            let existing = db.list_memberships_by_user(&user_id).await?;
            if existing.iter().any(|m| m.instance_id == instance) {
                return Err(anyhow!(
                    "user {user_id} is already a member of instance {instance} — remove it first \
                     to change the role"
                ));
            }
            let role: MembershipRole = role.into();
            if dry_run {
                println!(
                    "[dry-run] would add user {user_id} to instance {instance} as {}",
                    role.as_str()
                );
                return Ok(());
            }
            let membership = db.create_membership(&user_id, &instance, role).await?;
            println!(
                "id: {}\nuser_id: {}\ninstance_id: {}\nrole: {}",
                membership.id,
                membership.user_id,
                membership.instance_id,
                membership.role.as_str()
            );
        }
        MemberCmd::Remove { instance, user } => {
            let user_id = resolve_user_id(db, &user).await?;
            let membership = db
                .list_memberships_by_user(&user_id)
                .await?
                .into_iter()
                .find(|m| m.instance_id == instance)
                .ok_or_else(|| anyhow!("user {user_id} is not a member of instance {instance}"))?;
            if dry_run {
                println!(
                    "[dry-run] would remove membership {} (user {user_id}, instance {instance})",
                    membership.id
                );
                return Ok(());
            }
            db.delete_membership(&membership.id).await?;
            println!("removed membership {}", membership.id);
        }
        MemberCmd::List { instance } => {
            let memberships = db.list_memberships_by_instance(&instance).await?;
            if memberships.is_empty() {
                println!("0 member(s)");
                return Ok(());
            }
            let user_ids: Vec<&str> = memberships.iter().map(|m| m.user_id.as_str()).collect();
            let users = db.get_users(&user_ids).await?;
            println!("{:>12}  {:<30}  {:<7}", "user_id", "email", "role");
            for (m, user) in memberships.iter().zip(users) {
                let email = user
                    .map(|u| u.email)
                    .unwrap_or_else(|| "<missing>".to_string());
                println!("{:<12}  {:<30}  {:<7}", m.user_id, email, m.role.as_str());
            }
            println!("{} member(s)", memberships.len());
        }
    }
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    microticket::load_cli_env();

    let cli = Cli::parse();

    let db_prefix = cli
        .db_prefix
        .clone()
        .or_else(|| std::env::var("DB_PREFIX").ok())
        .ok_or_else(|| anyhow!("DB_PREFIX is required (--db-prefix or the DB_PREFIX env var)"))?;

    // read_only mirrors --dry-run: even if a bug somewhere below tried to
    // write during a dry run, the handler itself would refuse.
    let db = dynamodb::Handler::new(&db_prefix, cli.dry_run).await;

    match cli.object {
        Object::Instance { cmd } => run_instance(&db, cmd, cli.dry_run).await,
        Object::Address { cmd } => run_address(&db, cmd, cli.dry_run).await,
        Object::User { cmd } => run_user(&db, cmd, cli.dry_run).await,
        Object::Member { cmd } => run_member(&db, cmd, cli.dry_run).await,
    }
}
