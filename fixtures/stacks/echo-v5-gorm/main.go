package main

import (
	"log/slog"
	"net/http"

	"github.com/labstack/echo/v5"
	"github.com/labstack/echo/v5/middleware"
	"gorm.io/gorm"
)

// db is the database connection the handlers share.
var db *gorm.DB

func main() {
	e := echo.New()

	e.Use(middleware.RequestLogger()) // log requests with log/slog
	e.Use(middleware.Recover())       // recover from panics and return an error

	e.GET("/", hello)

	if err := e.Start(":8080"); err != nil {
		slog.Error("failed to start server", "error", err)
	}
}

func hello(c *echo.Context) error {
	return c.String(http.StatusOK, "Hello, World!")
}
