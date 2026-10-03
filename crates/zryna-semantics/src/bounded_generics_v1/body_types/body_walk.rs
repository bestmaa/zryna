use zryna_syntax::v4::RawStatementKind;
use zryna_syntax::v5::RawExpressionKind;

use super::super::DeclarationIdentity;
use super::value_names::{Binding, Scope};
use super::{
    BodyTypeFailure, Checker, expressions, matches, resources, statements, type_resolution,
};

enum Work<'a> {
    Block(u32, Option<Binding<'a>>),
    Leave,
    Statement(u32),
    FinishStatement(u32),
    Expression(u32),
    FinishExpression(u32),
    PrepareMatch(u32),
    Arm(u32, usize),
}
pub(super) fn check(
    checker: &mut Checker<'_, '_>,
    owner: DeclarationIdentity,
) -> Result<(), BodyTypeFailure> {
    let context = checker.context;
    let function = resources::raw_function(context, owner);
    let (mut scope, mut work) = storage(function, checker.tables.function(owner).arms.len())?;
    scope.enter();
    for parameter in &function.parameters {
        let ty = type_resolution::source(checker, owner.module().index(), parameter.type_syntax);
        scope.bind(checker, Binding { name: &parameter.name, ty });
    }
    // Function parameters and the outermost body share the original function scope.
    for statement in function.body.blocks[function.body.root_block as usize].statements.iter().rev()
    {
        work.push(Work::Statement(*statement));
    }
    while let Some(task) = work.pop() {
        match task {
            Work::Block(index, binding) => {
                scope.enter();
                if let Some(binding) = binding {
                    scope.bind(checker, binding);
                }
                work.push(Work::Leave);
                for statement in function.body.blocks[index as usize].statements.iter().rev() {
                    work.push(Work::Statement(*statement));
                }
            }
            Work::Leave => scope.leave(),
            Work::Statement(index) => {
                let statement = &function.body.statements[index as usize];
                work.push(Work::FinishStatement(index));
                match &statement.kind {
                    RawStatementKind::LocalDeclaration { initializer, .. } => {
                        work.push(Work::Expression(*initializer));
                    }
                    RawStatementKind::Assignment { target, value, .. } => {
                        work.push(Work::Expression(*value));
                        work.push(Work::Expression(*target));
                    }
                    RawStatementKind::Return { value, .. } => work.push(Work::Expression(*value)),
                    RawStatementKind::Block { .. } => {}
                    RawStatementKind::If { condition, .. }
                    | RawStatementKind::While { condition, .. } => {
                        work.push(Work::Expression(*condition));
                    }
                    RawStatementKind::ExpressionStatement { expression, .. } => {
                        work.push(Work::Expression(*expression));
                    }
                    RawStatementKind::WeakUpgrade { weak, .. } => {
                        work.push(Work::Expression(*weak));
                    }
                }
            }
            Work::FinishStatement(index) => {
                let statement = &function.body.statements[index as usize];
                statements::check(checker, owner, index, &mut scope)?;
                blocks(checker, owner, index, statement, &mut work);
            }
            Work::Expression(index) => {
                let expression = &function.body.expressions[index as usize];
                if let RawExpressionKind::Match { scrutinee, .. } = expression.kind {
                    work.push(Work::FinishExpression(index));
                    work.push(Work::PrepareMatch(index));
                    work.push(Work::Expression(scrutinee));
                } else {
                    work.push(Work::FinishExpression(index));
                    expressions::children(&expression.kind, |child| {
                        work.push(Work::Expression(child));
                    });
                }
            }
            Work::FinishExpression(index) => expressions::check(checker, owner, index, &scope)?,
            Work::PrepareMatch(index) => {
                matches::prepare(checker, owner, index)?;
                let RawExpressionKind::Match { arms, .. } =
                    &function.body.expressions[index as usize].kind
                else {
                    return Err(BodyTypeFailure::InternalFailure);
                };
                for arm in (0..arms.len()).rev() {
                    work.push(Work::Arm(index, arm));
                }
            }
            Work::Arm(index, arm) => {
                let RawExpressionKind::Match { arms, .. } =
                    &function.body.expressions[index as usize].kind
                else {
                    return Err(BodyTypeFailure::InternalFailure);
                };
                scope.enter();
                if let Some(name) = &arms[arm].binding {
                    let ty = matches::payload(checker, owner, index, arm)?;
                    scope.bind(checker, Binding { name, ty });
                }
                work.push(Work::Leave);
                work.push(Work::Expression(arms[arm].value));
            }
        }
    }
    Ok(())
}

fn blocks<'a>(
    checker: &Checker<'_, '_>,
    owner: DeclarationIdentity,
    index: u32,
    statement: &'a zryna_syntax::v4::RawStatementSyntax,
    work: &mut Vec<Work<'a>>,
) {
    match &statement.kind {
        RawStatementKind::Block { block } => work.push(Work::Block(*block, None)),
        RawStatementKind::If { then_block, else_clause, .. } => {
            if let Some(clause) = else_clause {
                work.push(Work::Block(clause.block, None));
            }
            work.push(Work::Block(*then_block, None));
        }
        RawStatementKind::While { body_block, .. } => {
            work.push(Work::Block(*body_block, None));
        }
        RawStatementKind::WeakUpgrade { binding, success_block, failure_block, .. } => {
            let ty = checker.tables.function(owner).statements[index as usize].ty;
            work.push(Work::Block(*failure_block, None));
            work.push(Work::Block(*success_block, Some(Binding { name: binding, ty })));
        }
        RawStatementKind::LocalDeclaration { .. }
        | RawStatementKind::Assignment { .. }
        | RawStatementKind::Return { .. }
        | RawStatementKind::ExpressionStatement { .. } => {}
    }
}

fn storage(
    function: &zryna_syntax::v5::RawFunctionSyntax,
    arms: usize,
) -> Result<(Scope<'_>, Vec<Work<'_>>), BodyTypeFailure> {
    let bindings = resources::checked_add(
        function.parameters.len(),
        resources::checked_add(function.body.statements.len(), arms)?,
    )?;
    // Every frame belongs to an original block, statement, expression or arm. No runtime
    // branch iteration or recursive function-body expansion is performed.
    let tasks = resources::checked_add(
        function.body.blocks.len(),
        resources::checked_add(
            function.body.statements.len(),
            resources::checked_add(function.body.expressions.len(), arms)?,
        )?,
    )?;
    let scope = Scope::new(bindings, tasks + 1)?;
    let work = resources::reserve(tasks * 3 + 1)?;
    Ok((scope, work))
}
