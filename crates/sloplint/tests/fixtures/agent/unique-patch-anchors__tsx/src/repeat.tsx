export function stepFirst(input: number): number {
  const alphaValue = computeFirst(input);
  const betaValue = computeSecond(alphaValue);
  const gammaValue = computeThird(betaValue);
  const deltaValue = computeFourth(gammaValue);
  const epsilonValue = computeFifth(deltaValue);
  storeResult(epsilonValue);
  return finishFirst();
}
export function stepSecond(input: number): number {
  const alphaValue = computeFirst(input);
  const betaValue = computeSecond(alphaValue);
  const gammaValue = computeThird(betaValue);
  const deltaValue = computeFourth(gammaValue);
  const epsilonValue = computeFifth(deltaValue);
  storeResult(epsilonValue);
  return finishSecond();
}
export function stepThird(input: number): number {
  const alphaValue = computeFirst(input);
  const betaValue = computeSecond(alphaValue);
  const gammaValue = computeThird(betaValue);
  const deltaValue = computeFourth(gammaValue);
  const epsilonValue = computeFifth(deltaValue);
  storeResult(epsilonValue);
  return finishThird();
}
function computeFirst(n: number): number {
  return n;
}
function computeSecond(n: number): number {
  return n;
}
function computeThird(n: number): number {
  return n;
}
function computeFourth(n: number): number {
  return n;
}
function computeFifth(n: number): number {
  return n;
}
function storeResult(n: number): void {
}
function finishFirst(): number {
  return 1;
}
function finishSecond(): number {
  return 2;
}
function finishThird(): number {
  return 3;
}
