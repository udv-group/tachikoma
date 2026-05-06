package main

import (
	"database/sql"
	"fmt"

	_ "github.com/lib/pq"
)

func connectToDB(dbparams DBParams) (*sql.DB, error) {
	connectionString := fmt.Sprintf("host=%s port=%d user=%s password=%s dbname=%s sslmode=disable", dbparams.Host, dbparams.Port, dbparams.User, dbparams.Password, dbparams.DBname)

	dbConn, err := sql.Open("postgres", connectionString)
	if err != nil {
		return nil, fmt.Errorf("goAPI failed to connect to DB: %v", err)
	}

	// проверка соединения
	if err = dbConn.Ping(); err != nil {
		dbConn.Close()
		return nil, fmt.Errorf("goAPI failed to ping to DB: %v", err)
	}

	fmt.Println("[GO API] Successfully connected to the database!")
	return dbConn, nil
}

func addNewHostToDB(dbConn *sql.DB, params Params) error {
	query := fmt.Sprintf("INSERT INTO %s(hostname, ip_address, groupid) VALUES ($1, $2, $3);", params.TableName)

	_, err := dbConn.Exec(query, params.Hostname, params.HostIP, params.HostGroup)
	if err != nil {
		return fmt.Errorf("failed to insert row into %s: %v", params.TableName, err)
	}

	fmt.Printf("[GO API] Successfully inserted new value %s in %s\n", params.HostIP, params.TableName)
	return nil
}

func deleteHostFromDB(dbConn *sql.DB, params Params) error {
	query := fmt.Sprintf("DELETE FROM %s WHERE ip_address='%s';", params.TableName, params.HostIP)

	_, err := dbConn.Exec(query)
	if err != nil {
		return fmt.Errorf("failed to delete row in %s: %v", params.TableName, err)
	}

	fmt.Printf("[GO API] Successfully deleted host with IP(%s) in %s\n", params.HostIP, params.TableName)
	return nil
}
