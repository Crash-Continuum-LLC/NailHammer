REM A mini BASIC. Case does not matter: FOR and for are the same word.

PRINT "times table"
PRINT

FOR row = 1 TO 3
  FOR col = 1 TO 3
    PRINT row, col, row * col
  NEXT col
NEXT row

PRINT
PRINT "counting down"

FOR n = 5 TO 1 STEP -2
  PRINT n
NEXT n

REM The counter is left one step past the limit, as BASIC does.
PRINT "n ended at", n

REM A loop whose range is empty runs its body zero times. That is only
REM possible because the body arrives unevaluated.
FOR skipped = 10 TO 1
  PRINT "never printed"
NEXT

Total = 0
FOR i = 1 TO 10
  Total = total + i
NEXT i
PRINT "sum 1..10 is", TOTAL

REM WHILE defers its condition as well as its body, so the test is re-run.
countdown = 3
WHILE countdown > 0
  PRINT "t minus", countdown
  countdown = countdown - 1
WEND

REM A subroutine keeps its body and runs it at every CALL. This is the one
REM construct that needed the tree to be owned rather than borrowed.
SUB banner
  PRINT "---------------"
END SUB

CALL banner

REM EXIT and CONTINUE are signals named after the construct they leave, so an
REM EXIT FOR raised inside the WHILE passes through it to the loop that owns it.
PRINT "primes under 20, the hard way"
FOR n = 2 TO 19
  d = 2
  divides = 0
  WHILE d < n
    IF n MOD d = 0 THEN divides = 1
    IF divides = 1 THEN EXIT WHILE
    d = d + 1
  WEND
  IF divides = 1 THEN CONTINUE FOR
  PRINT n
NEXT n

REM A function: parameters, a return value, and callable inside an expression.
REM Each call gets its own frame, which is what makes recursion work.
FUNCTION fact(n)
  IF n <= 1 THEN RETURN 1
  RETURN n * fact(n - 1)
END FUNCTION

CALL banner
PRINT "6! =", fact(6)
PRINT "and it folds like any operand:", fact(3) + fact(2) * 2

CALL banner
PRINT "17 MOD 5 =", 17 MOD 5
PRINT "3 < 4 AND NOT 0 =", 3 < 4 AND NOT 0
