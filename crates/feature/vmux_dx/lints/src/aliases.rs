use std::collections::{HashMap, HashSet};

use rustc_hir::def::Res;
use rustc_hir::intravisit::{self, Visitor};
use rustc_hir::{Arm, Expr, ExprKind, HirId, QPath};
use rustc_lint::LateContext;

pub(super) struct Assignments<'tcx> {
    pub values: Vec<&'tcx Expr<'tcx>>,
    binding: Option<HirId>,
    target: Option<HirId>,
    observed: Vec<&'tcx Expr<'tcx>>,
    reachable: bool,
    breaks: HashMap<HirId, Vec<&'tcx Expr<'tcx>>>,
    continues: HashMap<HirId, Vec<&'tcx Expr<'tcx>>>,
    exits: HashMap<HirId, Vec<&'tcx Expr<'tcx>>>,
}

impl<'tcx> Assignments<'tcx> {
    pub fn of(
        cx: &LateContext<'tcx>,
        expression: &Expr<'tcx>,
        binding: HirId,
        initializer: Option<&'tcx Expr<'tcx>>,
    ) -> Self {
        let mut assignments = Self::new(Some(binding), Some(expression.hir_id));
        if let Some(initializer) = initializer {
            assignments.values.push(initializer);
        }
        if let Some(body) = cx.tcx.hir_maybe_body_owned_by(binding.owner.def_id) {
            assignments.visit_body(body);
            assignments.values = std::mem::take(&mut assignments.observed);
        }
        assignments
    }

    pub fn exits(expression: &'tcx Expr<'tcx>) -> Vec<&'tcx Expr<'tcx>> {
        let mut assignments = Self::new(None, None);
        assignments.visit_expr(expression);
        assignments
            .exits
            .remove(&expression.hir_id)
            .unwrap_or_default()
    }

    fn new(binding: Option<HirId>, target: Option<HirId>) -> Self {
        Self {
            values: Vec::new(),
            binding,
            target,
            observed: Vec::new(),
            reachable: true,
            breaks: HashMap::new(),
            continues: HashMap::new(),
            exits: HashMap::new(),
        }
    }

    fn branches(
        &mut self,
        condition: &'tcx Expr<'tcx>,
        then: &'tcx Expr<'tcx>,
        otherwise: Option<&'tcx Expr<'tcx>>,
    ) {
        self.visit_expr(condition);
        if !self.reachable {
            return;
        }
        let before = self.values.clone();
        self.visit_expr(then);
        let then = if self.reachable {
            Some(self.values.clone())
        } else {
            None
        };
        self.values = before;
        self.reachable = true;
        if let Some(otherwise) = otherwise {
            self.visit_expr(otherwise);
        }
        if let Some(then) = then {
            if self.reachable {
                Self::merge(&mut self.values, then);
            } else {
                self.values = then;
                self.reachable = true;
            }
        }
    }

    fn alternatives(&mut self, scrutinee: &'tcx Expr<'tcx>, arms: &'tcx [Arm<'tcx>]) {
        self.visit_expr(scrutinee);
        if !self.reachable {
            return;
        }
        let before = self.values.clone();
        let mut combined = Vec::new();
        let mut reachable = false;
        for arm in arms {
            self.values.clone_from(&before);
            self.reachable = true;
            if let Some(guard) = arm.guard {
                self.visit_expr(guard);
            }
            self.visit_expr(arm.body);
            if self.reachable {
                reachable = true;
                Self::merge(&mut combined, self.values.clone());
            }
        }
        self.values = combined;
        self.reachable = reachable;
    }

    fn repeat(&mut self, expression: &'tcx Expr<'tcx>, body: &'tcx rustc_hir::Block<'tcx>) {
        let target = expression.hir_id;
        let mut entry = self.values.clone();
        loop {
            let previous: HashSet<_> = entry.iter().map(|value| value.hir_id).collect();
            self.values.clone_from(&entry);
            self.reachable = true;
            self.continues.remove(&target);
            self.visit_block(body);
            if self.reachable {
                Self::merge(&mut entry, self.values.clone());
            }
            if let Some(values) = self.continues.remove(&target) {
                Self::merge(&mut entry, values);
            }
            let current: HashSet<_> = entry.iter().map(|value| value.hir_id).collect();
            if current == previous {
                break;
            }
        }
        self.reachable = self.breaks.contains_key(&target);
        self.values = self.breaks.remove(&target).unwrap_or_default();
    }

    fn merge(values: &mut Vec<&'tcx Expr<'tcx>>, additional: Vec<&'tcx Expr<'tcx>>) {
        values.extend(additional);
        let mut seen = HashSet::new();
        values.retain(|value| seen.insert(value.hir_id));
    }
}

impl<'tcx> Visitor<'tcx> for Assignments<'tcx> {
    fn visit_expr(&mut self, expression: &'tcx Expr<'tcx>) {
        if !self.reachable {
            return;
        }
        if self.target == Some(expression.hir_id) {
            Self::merge(&mut self.observed, self.values.clone());
        }
        match expression.kind {
            ExprKind::If(condition, then, otherwise) => self.branches(condition, then, otherwise),
            ExprKind::Match(scrutinee, arms, _) => self.alternatives(scrutinee, arms),
            ExprKind::Loop(body, ..) => self.repeat(expression, body),
            ExprKind::Break(destination, value) => {
                if let Some(value) = value {
                    self.visit_expr(value);
                }
                if self.reachable
                    && let Ok(target) = destination.target_id
                {
                    Self::merge(self.breaks.entry(target).or_default(), self.values.clone());
                    if let Some(value) = value {
                        Self::merge(self.exits.entry(target).or_default(), vec![value]);
                    }
                }
                self.reachable = false;
            }
            ExprKind::Continue(destination) => {
                if let Ok(target) = destination.target_id {
                    Self::merge(
                        self.continues.entry(target).or_default(),
                        self.values.clone(),
                    );
                }
                self.reachable = false;
            }
            ExprKind::Ret(value) => {
                if let Some(value) = value {
                    self.visit_expr(value);
                }
                self.reachable = false;
            }
            ExprKind::Assign(left, right, _) => {
                self.visit_expr(left);
                self.visit_expr(right);
                if self.reachable
                    && let ExprKind::Path(QPath::Resolved(_, path)) = left.kind
                    && matches!(path.res, Res::Local(binding) if self.binding == Some(binding))
                {
                    self.values = vec![right];
                }
            }
            _ => intravisit::walk_expr(self, expression),
        }
    }
}
