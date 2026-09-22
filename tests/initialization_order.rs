//! R06: observe dependency layers through the public registration/build API.
#![forbid(unsafe_code)]

trait IConfig {}
impl IConfig for u32 {}
trait IDatabase {}
impl IDatabase for u32 {}
trait IRepository {}
impl IRepository for u32 {}
trait IAudit {}
impl IAudit for u32 {}
trait IMetrics {}
impl IMetrics for u32 {}
trait IService {}
impl IService for u32 {}
impl IService for (u32,) {}

mod layers {
    use super::*;

    #[systasis::container(require(Send, Sync))]
    #[test]
    fn build_freezes_layers_and_waits_for_the_deepest_dependency() {
        let mut events: Vec<&'static str> = Vec::new();
        let builder = systasis::systasis_container! {
            register_value!({
                events.push("service");
                resolve!(IConfig) + resolve!(IRepository)
            }: u32 as IService);
            register_value!({
                events.push("repository");
                resolve!(IDatabase) + resolve!(IConfig)
            }: u32 as IRepository);
            register_value!({ events.push("database"); 10 }: u32 as IDatabase);
            register_value!({
                events.push("audit");
                resolve!(IDatabase) * 2
            }: u32 as IAudit);
            register_value!({ events.push("config"); 2 }: u32 as IConfig);
            register_value!({ events.push("metrics"); 3 }: u32 as IMetrics);
        };
        let Ok(container) = builder.build();

        // Layer 0: database/config/metrics. Layer 1: repository/audit.
        // Service also uses config, but must wait for its deeper repository edge.
        assert_eq!(
            events,
            [
                "database",
                "config",
                "metrics",
                "repository",
                "audit",
                "service"
            ]
        );
        assert_eq!(container.resolve_i_service(), 14);
        assert_eq!(container.resolve_i_repository(), 12);
        assert_eq!(container.resolve_i_audit(), 20);
        assert_eq!(container.resolve_i_metrics(), 3);
    }
}

mod type_queries {
    use super::*;

    #[systasis::container]
    #[test]
    fn type_only_dependencies_order_initializers_without_resolving_values() {
        let mut events: Vec<&'static str> = Vec::new();
        let Ok(container) = systasis::systasis_container! {
            register_value!({
                events.push("service");
                (7,)
            }: (registered_type!(IDatabase),) as IService);
            register_value!({
                events.push("database");
                let value: resolve_type!(IConfig) = 5;
                value
            }: u32 as IDatabase);
            register_value!({ events.push("config"); 2 }: u32 as IConfig);
            register_value!({ events.push("metrics"); 3 }: u32 as IMetrics);
        }
        .build();

        assert_eq!(events, ["config", "metrics", "database", "service"]);
        assert_eq!(container.resolve_i_service(), (7,));
        assert_eq!(container.resolve_i_database(), 5);
        assert_eq!(container.resolve_i_config(), 2);
    }
}

mod overrides {
    use super::*;

    #[systasis::container]
    #[test]
    fn final_override_supplies_its_dependencies_and_source_position() {
        let mut events: Vec<&'static str> = Vec::new();
        let Ok(container) = systasis::systasis_container! {
            // If this superseded dependency were retained it would create a cycle.
            register_value!({
                events.push("discarded config");
                resolve!(IService)
            }: u32 as IConfig);
            register_value!({
                events.push("service");
                resolve!(IConfig) + resolve!(IDatabase)
            }: u32 as IService);
            register_value!({ events.push("database"); 10 }: u32 as IDatabase);
            register_value!({ events.push("config"); 2 }: u32 as IConfig);
        }
        .build();

        // The winning config appears after database, not in its old source slot.
        assert_eq!(events, ["database", "config", "service"]);
        assert_eq!(container.resolve_i_service(), 12);
        assert_eq!(container.resolve_i_config(), 2);
    }
}
