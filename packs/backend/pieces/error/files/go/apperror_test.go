package apperror

import (
	"errors"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
// @cuilo if stack.echo

	"github.com/labstack/echo/v4" // @cuilo if !stack.echo-v5
	"github.com/labstack/echo/v5" // @cuilo if stack.echo-v5
// @cuilo end
)

func TestProblemForKeepsApplicationErrors(t *testing.T) {
	problem := ProblemFor(NotFound("No invoice with id 42."))

	if problem.Status != http.StatusNotFound || problem.Detail != "No invoice with id 42." {
		t.Fatalf("got %+v", problem)
	}
}

func TestWriteHidesUnexpectedErrors(t *testing.T) {
	recorder := httptest.NewRecorder()

	Write(recorder, httptest.NewRequest(http.MethodGet, "/", nil), errors.New("password=hunter2"))

	if recorder.Code != http.StatusInternalServerError {
		t.Fatalf("status = %d", recorder.Code)
	}
	if strings.Contains(recorder.Body.String(), "hunter2") {
		t.Fatal("the error message leaked")
	}
	if got := recorder.Header().Get("Content-Type"); got != "application/problem+json" {
		t.Fatalf("content type = %q", got)
	}
}
// @cuilo if stack.echo

func TestHandlerSendsAnUnknownRouteAsA404Problem(t *testing.T) {
	app := echo.New()
	app.HTTPErrorHandler = Handler
	recorder := httptest.NewRecorder()

	app.ServeHTTP(recorder, httptest.NewRequest(http.MethodGet, "/missing", nil))

	if recorder.Code != http.StatusNotFound {
		t.Fatalf("status = %d", recorder.Code)
	}
	if got := recorder.Header().Get("Content-Type"); got != "application/problem+json" {
		t.Fatalf("content type = %q", got)
	}
}
// @cuilo end
