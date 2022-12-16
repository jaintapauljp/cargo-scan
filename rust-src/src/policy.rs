/*
    Audit policy language.

    Serializes to and deserializes from a .policy file.
    See example .policy files in policies/
*/

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::ffi::OsStr;
use std::fmt::{self, Display};
use std::path::Path;

use super::ident::{FnCall, Path as IdentPath};

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Statement {
    Allow { region: FnCall, effect: FnCall },
    Require { region: FnCall, effect: FnCall },
    Trust { region: FnCall },
}
impl Display for Statement {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Allow { region, effect } => {
                write!(f, "allow {} {}", region, effect)
            }
            Self::Require { region, effect } => {
                write!(f, "require {} {}", region, effect)
            }
            Self::Trust { region } => {
                write!(f, "trust {}", region)
            }
        }
    }
}
impl Statement {
    pub fn allow_simple(path: &str, effect: &str) -> Self {
        let region = FnCall::new_all(path);
        let effect = FnCall::new_all(effect);
        Self::Allow { region, effect }
    }
    pub fn require_simple(path: &str, effect: &str) -> Self {
        let region = FnCall::new_all(path);
        let effect = FnCall::new_all(effect);
        Self::Require { region, effect }
    }
    pub fn allow(path: &str, args: &str, effect: FnCall) -> Self {
        let region = FnCall::new(path, args);
        Self::Allow { region, effect }
    }
    pub fn require(path: &str, args: &str, effect: FnCall) -> Self {
        let region = FnCall::new(path, args);
        Self::Require { region, effect }
    }
    pub fn trust(path: &str) -> Self {
        let region = FnCall::new_all(path);
        Self::Trust { region }
    }
}

// TODO: make crate_version and policy_version semver objects
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Policy {
    crate_name: String,
    crate_version: String,
    policy_version: String,
    statements: Vec<Statement>,
}
impl Policy {
    pub fn new(crate_name: &str, crate_version: &str, policy_version: &str) -> Self {
        let crate_name = crate_name.to_string();
        let crate_version = crate_version.to_string();
        let policy_version = policy_version.to_string();
        let statements = Vec::new();
        Policy { crate_name, crate_version, policy_version, statements }
    }
    pub fn from_file(file: &Path) -> Result<Self, Box<dyn Error>> {
        debug_assert_eq!(file.extension(), Some(OsStr::new("toml")));
        let toml_str = std::fs::read_to_string(file)?;
        let policy: Policy = toml::from_str(&toml_str)?;
        Ok(policy)
    }
    pub fn add_statement(&mut self, s: Statement) {
        self.statements.push(s);
    }
    pub fn allow_simple(&mut self, path: &str, effect: &str) {
        self.add_statement(Statement::allow_simple(path, effect));
    }
    pub fn require_simple(&mut self, path: &str, effect: &str) {
        self.add_statement(Statement::require_simple(path, effect));
    }
    pub fn allow(&mut self, path: &str, args: &str, eff: FnCall) {
        self.add_statement(Statement::allow(path, args, eff))
    }
    pub fn require(&mut self, path: &str, args: &str, eff: FnCall) {
        self.add_statement(Statement::require(path, args, eff))
    }
    pub fn trust(&mut self, path: &str) {
        self.add_statement(Statement::trust(path))
    }
}

/// Quick-lookup summary of the policy.
/// Note: may make more sense to merge these fields into Policy eventually; current separate
/// because would require custom serialization/deserialization logic.
#[allow(dead_code, unused_variables)]
#[derive(Debug)]
pub struct PolicyLookup {
    allow_sets: HashMap<IdentPath, HashSet<IdentPath>>,
    require_sets: HashMap<IdentPath, HashSet<IdentPath>>,
}
#[allow(dead_code, unused_variables)]
impl PolicyLookup {
    pub fn empty() -> Self {
        Self { allow_sets: HashMap::new(), require_sets: HashMap::new() }
    }
    pub fn from_policy(p: &Policy) -> Self {
        let mut result = Self::empty();
        for stmt in &p.statements {
            result.add_statement(stmt);
        }
        result
    }
    pub fn add_statement(&mut self, stmt: &Statement) {
        match stmt {
            Statement::Allow { region: r, effect: e } => {
                let caller = r.fn_path().clone();
                let eff = e.fn_path().clone();
                self.allow_sets.entry(caller).or_default().insert(eff);
            }
            Statement::Require { region: r, effect: e } => {
                let caller = r.fn_path().clone();
                let eff = e.fn_path().clone();
                self.require_sets.entry(caller).or_default().insert(eff);
                // require encompasses allow
                let caller = r.fn_path().clone();
                let eff = e.fn_path().clone();
                self.allow_sets.entry(caller).or_default().insert(eff);
            }
            Statement::Trust { region: _ } => {
                unimplemented!()
            }
        }
    }
    /// Mark a fn call is an interesting/dangerous call.
    /// This must be done before any check_edge invocations.
    ///
    /// We re-use the require list for this, since it serves the same purpose!
    pub fn mark_of_interest(&mut self, callee: &IdentPath) {
        self.require_sets.entry(callee.clone()).or_default().insert(callee.clone());
    }

    // internal function for check_edge
    fn allow_list_contains(
        &self,
        caller: &IdentPath,
        effect: &IdentPath,
    ) -> Result<(), String> {
        if let Some(allow) = self.allow_sets.get(caller) {
            if allow.contains(effect) {
                Ok(())
            } else {
                Err(format!(
                    "Allow list for function {} missing effect {}",
                    caller, effect
                ))
            }
        } else {
            Err(format!("No allow list for function {} with effect {}", caller, effect))
        }
    }

    /// Iterate over effects required at a particular path
    pub fn iter_requirements(
        &self,
        callee: &IdentPath,
    ) -> impl Iterator<Item = &IdentPath> {
        self.require_sets.get(callee).into_iter().flat_map(|require| require.iter())
    }

    /// Check a call graph edge against the policy.
    /// Currently, edges can be read in in any order; the lookup does
    /// not need any particular order. This may change later.
    pub fn check_edge(
        &self,
        caller: &IdentPath,
        callee: &IdentPath,
        error_list: &mut Vec<String>,
    ) {
        for req in self.iter_requirements(callee) {
            self.allow_list_contains(caller, req).unwrap_or_else(|err| {
                error_list.push(err);
            });
        }
    }

    /// Check a call graph edge against the policy.
    /// Rather than returning a list of errors, just return a Boolean
    /// of whether it passes or not.
    pub fn check_edge_bool(&self, caller: &IdentPath, callee: &IdentPath) -> bool {
        for req in self.iter_requirements(callee) {
            if self.allow_list_contains(caller, req).is_err() {
                return false;
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_policy_serialize_deserialize() {
        // Note: this example uses dummy strings that don't correspond
        // to real effects
        let cr = "permissions-ex";
        let mut policy = Policy::new(cr, "0.1", "0.1");
        let eff1 = FnCall::new("fs::delete", "path");
        policy.require("permissions-ex::lib::remove", "path", eff1);
        let eff2 = FnCall::new("fs::create", "path");
        policy.require("permissions-ex::lib::save_data", "path", eff2);
        let eff3 = FnCall::new("fs::write", "path");
        policy.require("permissions-ex::lib::save_data", "path", eff3);
        let eff4 = FnCall::new("process::exec", "rm -f path");
        policy.allow("permissions-ex::lib::remove", "path", eff4);
        let eff5 = FnCall::new("fs::delete", "path");
        policy.allow("permissions-ex::lib::save_data", "path", eff5);
        let eff6 = FnCall::new("fs::append", "my_app.log");
        policy.allow("permissions-ex::lib::prepare_data", "", eff6);

        println!("Policy example: {:?}", policy);

        let policy_toml = toml::to_string(&policy).unwrap();
        println!("Policy serialized: {}", policy_toml);

        let policy2: Policy = toml::from_str(&policy_toml).unwrap();
        println!("Policy deserialized again: {:?}", policy2);

        assert_eq!(policy, policy2);
    }

    fn ex_policy() -> Policy {
        let cr = "ex";
        Policy::new(cr, "0.1", "0.1")
    }

    fn ex_lookup(policy: &Policy) -> PolicyLookup {
        let eff1 = IdentPath("libc::effect".to_string());
        let eff2 = IdentPath("std::effect".to_string());
        let mut lookup = PolicyLookup::from_policy(policy);
        lookup.mark_of_interest(&eff1);
        lookup.mark_of_interest(&eff2);
        lookup
    }

    #[test]
    fn test_policy_lookup_trivial() {
        let policy = ex_policy();
        let lookup = ex_lookup(&policy);

        let foo = IdentPath("foo".to_string());
        let bar = IdentPath("bar".to_string());
        let eff = IdentPath("std::effect".to_string());

        println!("{:?}", policy);
        println!("{:?}", lookup);

        // this should pass since it's just an edge between two random
        // non-effectful functions
        assert!(lookup.check_edge_bool(&foo, &bar));

        // this should fail since we haven't allowed the effect
        assert!(!lookup.check_edge_bool(&foo, &eff));
    }

    #[test]
    fn test_policy_lookup_allow() {
        let mut policy = ex_policy();
        policy.allow_simple("foo", "std::effect");
        let lookup = ex_lookup(&policy);

        let foo = IdentPath("foo".to_string());
        let bar = IdentPath("bar".to_string());
        let eff1 = IdentPath("std::effect".to_string());
        let eff2 = IdentPath("libc::effect".to_string());
        let eff3 = IdentPath("std::non_effect".to_string());

        println!("{:?}", policy);
        println!("{:?}", lookup);

        assert!(lookup.check_edge_bool(&foo, &eff1));
        assert!(!lookup.check_edge_bool(&foo, &eff2));
        assert!(lookup.check_edge_bool(&foo, &eff3));
        assert!(!lookup.check_edge_bool(&bar, &eff1));
        assert!(lookup.check_edge_bool(&bar, &foo));
    }

    #[test]
    fn test_policy_lookup_require() {
        let mut policy = ex_policy();
        policy.require_simple("foo", "std::effect");
        let lookup = ex_lookup(&policy);

        let foo = IdentPath("foo".to_string());
        let bar = IdentPath("bar".to_string());
        let eff1 = IdentPath("std::effect".to_string());
        let eff2 = IdentPath("libc::effect".to_string());

        println!("{:?}", policy);
        println!("{:?}", lookup);

        // Cases the same as test_policy_lookup_allow
        assert!(lookup.check_edge_bool(&foo, &eff1));
        assert!(!lookup.check_edge_bool(&foo, &eff2));
        assert!(!lookup.check_edge_bool(&bar, &eff1));
        // New case: can't have edge from foo to bar due to requirement
        // on callers of bar
        assert!(!lookup.check_edge_bool(&bar, &foo));
        // Reverse edge is OK
        assert!(lookup.check_edge_bool(&foo, &bar));
    }

    #[test]
    fn test_policy_lookup_1() {
        let mut policy = ex_policy();
        policy.allow_simple("foo::bar", "libc::effect");
        policy.allow_simple("foo::bar", "libc::non_effect");
        let lookup = ex_lookup(&policy);

        let bar = IdentPath("foo::bar".to_string());
        let eff1 = IdentPath("libc::effect".to_string());
        let eff2 = IdentPath("std::effect".to_string());
        let eff3 = IdentPath("libc::non_effect".to_string());
        let eff4 = IdentPath("std::non_effect".to_string());

        assert!(lookup.check_edge_bool(&bar, &eff1));
        assert!(!lookup.check_edge_bool(&bar, &eff2));
        assert!(lookup.check_edge_bool(&bar, &eff3));
        assert!(lookup.check_edge_bool(&bar, &eff4));
    }

    #[test]
    fn test_policy_lookup_2() {
        let mut policy = ex_policy();
        policy.allow_simple("foo::bar", "std::effect");
        policy.require_simple("foo::bar", "libc::effect");
        policy.require_simple("foo::f1", "libc::effect");
        policy.require_simple("foo::f2", "libc::effect");
        policy.allow_simple("foo::g1", "libc::effect");
        policy.allow_simple("foo::g2", "libc::effect");
        let lookup = ex_lookup(&policy);

        let bar = IdentPath::new("foo::bar");
        let f1 = IdentPath::new("foo::f1");
        let f2 = IdentPath::new("foo::f2");
        let g1 = IdentPath::new("foo::g1");
        let g2 = IdentPath::new("foo::g2");
        let g3 = IdentPath::new("foo::g3");
        let eff1 = IdentPath::new("libc::effect");
        let eff2 = IdentPath::new("std::effect");

        assert!(lookup.check_edge_bool(&bar, &eff1));
        assert!(lookup.check_edge_bool(&bar, &eff2));
        assert!(lookup.check_edge_bool(&f1, &bar));
        assert!(lookup.check_edge_bool(&f2, &f1));
        assert!(lookup.check_edge_bool(&g1, &f1));
        assert!(lookup.check_edge_bool(&g2, &f2));
        assert!(lookup.check_edge_bool(&g2, &f1));
        assert!(lookup.check_edge_bool(&g3, &g2));
        assert!(!lookup.check_edge_bool(&g3, &f1));
        assert!(!lookup.check_edge_bool(&g3, &f2));
    }

    #[test]
    fn test_policy_lookup_cycle() {
        // Interesting case involving cycles
        // I think this should be allowed but it's up for discussion
        // Solution is to mark program entrypoints that can't have
        // require statements

        // Notice: no allow statements
        let mut policy = ex_policy();
        policy.require_simple("foo", "libc::effect");
        policy.require_simple("bar", "libc::effect");
        let lookup = ex_lookup(&policy);

        let foo = IdentPath("foo".to_string());
        let bar = IdentPath("bar".to_string());

        assert!(lookup.check_edge_bool(&foo, &bar));
        assert!(lookup.check_edge_bool(&bar, &foo));
    }

    #[test]
    fn test_policy_from_file() {
        let policy_file = Path::new("../policies/permissions-ex.toml");
        let policy1 = Policy::from_file(policy_file).unwrap();

        let mut policy2 = Policy::new("permissions-ex", "0.1", "0.1");
        let eff1 = FnCall::new("fs::delete", "path");
        policy2.require("permissions-ex::remove", "path", eff1);
        let eff2 = FnCall::new("fs::create", "path");
        policy2.require("permissions-ex::save_data", "path", eff2);
        let eff3 = FnCall::new("fs::write", "path");
        policy2.require("permissions-ex::save_data", "path", eff3);
        let eff4 = FnCall::new("process::exec", "rm -f path");
        policy2.allow("permissions-ex::remove", "path", eff4);
        let eff5 = FnCall::new("fs::delete", "path");
        policy2.allow("permissions-ex::save_data", "path", eff5);
        let eff6 = FnCall::new("fs::append", "my_app.log");
        policy2.allow("permissions-ex::prepare_data", "", eff6);

        let policy1_toml = toml::to_string(&policy1).unwrap();
        let policy2_toml = toml::to_string(&policy2).unwrap();
        println!("policy 1: {:?} {}", policy1, policy1_toml);
        println!("policy 2: {:?} {}", policy2, policy2_toml);

        assert_eq!(policy1, policy2);
    }
}
