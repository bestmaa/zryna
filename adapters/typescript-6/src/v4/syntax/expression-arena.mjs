import {
  maxConstructionOperandsPerProject,
  maxExpressionsPerFunction,
  maxExpressionsPerProject,
} from '../boundary/configuration.mjs';
import {
  failBudget,
} from '../boundary/errors.mjs';

function pushExpression(context, expression) {
  if (context.expressions.length >= maxExpressionsPerFunction) failBudget('function exceeds the expression limit');
  context.budgets.expressions += 1;
  if (context.budgets.expressions > maxExpressionsPerProject) failBudget('project exceeds the expression limit');
  const id = context.expressions.length;
  context.expressions.push(expression);
  return id;
}

function countAggregateOperands(context, count) {
  context.budgets.aggregateOperands += count;
  if (context.budgets.aggregateOperands > maxConstructionOperandsPerProject) {
    failBudget('project exceeds the aggregate-construction operand limit');
  }
}

export {
  countAggregateOperands,
  pushExpression
};
