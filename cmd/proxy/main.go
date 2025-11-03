package main

import (
	"context"
	"log"
	"os"
	"os/signal"
	"syscall"

	"github.com/NoahCxrest/roblox-proxy-clustering/internal/app"
	"github.com/NoahCxrest/roblox-proxy-clustering/internal/config"
)

func main() {
	cfg, err := config.Load()
	if err != nil {
		log.Fatalf("load config: %v", err)
	}

	application, err := app.New(cfg)
	if err != nil {
		log.Fatalf("init app: %v", err)
	}

	ctx, stop := signal.NotifyContext(context.Background(), os.Interrupt, syscall.SIGTERM)
	defer stop()

	if err := application.Run(ctx); err != nil {
		log.Fatalf("run app: %v", err)
	}
}
