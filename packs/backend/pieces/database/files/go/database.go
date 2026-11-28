// Package database opens the PostgreSQL connection that repositories share.
package database

import (
	"errors"
	"os"

	"gorm.io/driver/postgres"
	"gorm.io/gorm"
)

// Open connects to the PostgreSQL server at DATABASE_URL.
func Open() (*gorm.DB, error) {
	url := os.Getenv("DATABASE_URL")
	if url == "" {
		return nil, errors.New("DATABASE_URL is not set")
	}
	return gorm.Open(postgres.Open(url), &gorm.Config{})
}
