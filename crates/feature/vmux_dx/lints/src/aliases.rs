extern crate rustc_span;

use std::collections::HashSet;

use rustc_hir::def::Res;
use rustc_hir::intravisit::{self, Visitor};
use rustc_hir::{Arm, Expr, ExprKind, HirId, QPath};
use rustc_lint::LateContext;
use rustc_span::BytePos;

pub(super) struct Assignments<'tcx> {
    pub values: Vec<&'tcx Expr<'tcx>>,
    binding: HirId,
    before: BytePos,
    target_ancestors: HashSet<HirId>,
}

impl<'tcx> Assignments<'tcx> {
    pub fn of(
        cx: &LateContext<'tcx>,
        expression: &Expr<'tcx>,
        binding: HirId,
        initializer: Option<&'tcx Expr<'tcx>>,
    ) -> Self {
        let mut values = Vec::new();
        if let Some(initializer) = initializer {
            values.push(initializer);
        }
        let mut target_ancestors = HashSet::from([expression.hir_id]);
        for (parent, _) in cx.tcx.hir_parent_iter(expression.hir_id) {
            target_ancestors.insert(parent);
        }
        let mut assignments = Self {
            values,
            binding,
            before: expression.span.lo(),
            target_ancestors,
        };
        if let Some(body) = cx.tcx.hir_maybe_body_owned_by(binding.owner.def_id) {
            assignments.visit_body(body);
        }
        assignments
    }

    fn branches(
        &mut self,
        condition: &'tcx Expr<'tcx>,
        then: &'tcx Expr<'tcx>,
        otherwise: Option<&'tcx Expr<'tcx>>,
    ) {
        self.visit_expr(condition);
        if condition.span.hi() > self.before {
            return;
        }
        if self.target_ancestors.contains(&then.hir_id) {
            self.visit_expr(then);
            return;
        }
        if let Some(otherwise) = otherwise
            && self.target_ancestors.contains(&otherwise.hir_id)
        {
            self.visit_expr(otherwise);
            return;
        }
        let before = self.values.clone();
        self.visit_expr(then);
        let then = std::mem::replace(&mut self.values, before);
        if let Some(otherwise) = otherwise {
            self.visit_expr(otherwise);
        }
        self.merge(then);
    }

    fn alternatives(&mut self, scrutinee: &'tcx Expr<'tcx>, arms: &'tcx [Arm<'tcx>]) {
        self.visit_expr(scrutinee);
        if scrutinee.span.hi() > self.before {
            return;
        }
        for arm in arms {
            if self.target_ancestors.contains(&arm.hir_id) {
                if let Some(guard) = arm.guard {
                    self.visit_expr(guard);
                }
                self.visit_expr(arm.body);
                return;
            }
        }
        let before = self.values.clone();
        let mut combined = Vec::new();
        for arm in arms {
            self.values.clone_from(&before);
            if let Some(guard) = arm.guard {
                self.visit_expr(guard);
            }
            self.visit_expr(arm.body);
            combined.append(&mut self.values);
        }
        self.merge(combined);
    }

    fn merge(&mut self, alternatives: Vec<&'tcx Expr<'tcx>>) {
        let mut seen = HashSet::new();
        self.values.extend(alternatives);
        self.values.retain(|value| seen.insert(value.hir_id));
    }
}

impl<'tcx> Visitor<'tcx> for Assignments<'tcx> {
    fn visit_expr(&mut self, expression: &'tcx Expr<'tcx>) {
        if expression.span.lo() >= self.before {
            return;
        }
        match expression.kind {
            ExprKind::If(condition, then, otherwise) => {
                self.branches(condition, then, otherwise);
            }
            ExprKind::Match(scrutinee, arms, _) => self.alternatives(scrutinee, arms),
            ExprKind::Loop(body, ..) => {
                let before = self.values.clone();
                intravisit::walk_expr(self, expression);
                if !self.target_ancestors.contains(&body.hir_id) {
                    self.merge(before);
                }
            }
            ExprKind::Assign(left, right, _) => {
                intravisit::walk_expr(self, expression);
                if expression.span.hi() <= self.before
                    && let ExprKind::Path(QPath::Resolved(_, path)) = left.kind
                    && path.res == Res::Local(self.binding)
                {
                    self.values.clear();
                    self.values.push(right);
                }
            }
            _ => intravisit::walk_expr(self, expression),
        }
    }
}
